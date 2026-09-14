import {
  DndContext,
  KeyboardSensor,
  PointerSensor,
  closestCorners,
  useSensor,
  useSensors,
  DragEndEvent,
  DragOverEvent,
} from "@dnd-kit/core";
import { arrayMove, sortableKeyboardCoordinates } from "@dnd-kit/sortable";
import { Trans, useLingui } from "@lingui/react/macro";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  EyeIcon,
  FilePenLineIcon,
  FolderPlusIcon,
  ListIcon,
  PencilIcon,
  PlusIcon,
  SaveIcon,
  Trash2Icon,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { toast } from "sonner";

import { errorMessage, isConflictError } from "@/api/client";
import { useInvalidateLists } from "@/api/invalidate-lists";
import { addItemToList, removeList, saveList } from "@/api/lists";
import { listQuery, queryKeys } from "@/api/queries";
import { MarkdownView } from "@/components/assets/markdown-view";
import { AppFrame } from "@/components/layout/app-frame";
import { PageContainer } from "@/components/layout/page-container";
import { AddItemsDialog } from "@/components/lists/add-items-dialog";
import {
  sectionsSignature,
  serverSections,
  type EditableSection,
} from "@/components/lists/list-sections";
import { SectionBlock } from "@/components/lists/section-block";
import { RenameDialog } from "@/components/rename-dialog";
import { Alert } from "@/components/ui/alert";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Placeholder } from "@/components/ui/placeholder";
import { Textarea } from "@/components/ui/textarea";
import { useDebouncedCallback } from "@/hooks/use-debounce";
import { useCapabilities } from "@/lib/capabilities";
import { useTitleLanguage } from "@/lib/language";

export function ListDetailPage() {
  const { t } = useLingui();
  const { id = "" } = useParams();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const invalidateLists = useInvalidateLists();
  const language = useTitleLanguage();

  const list = useQuery({ ...listQuery(id), enabled: Boolean(id) });
  const capabilities = useCapabilities();
  const contentWritable = capabilities.contentWritable;

  const [sections, setSections] = useState<EditableSection[]>([]);
  // Mirrors `sections` for the debounced toggle auto-save, whose timer fires after
  // its scheduling render — reading the ref keeps it on the latest state.
  const sectionsRef = useRef(sections);
  sectionsRef.current = sections;
  const [description, setDescription] = useState("");
  const [trailing, setTrailing] = useState("");
  const [addOpen, setAddOpen] = useState(false);
  const [renameOpen, setRenameOpen] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [externalChange, setExternalChange] = useState(false);
  // Monotonic counter for fresh React keys on locally-created sections/items, so
  // dnd-kit identities stay stable across edits without colliding with `srv-*`.
  const keyCounter = useRef(0);
  const nextKey = (prefix: string) => `${prefix}-${(keyCounter.current += 1)}`;
  // Re-sync local edit state from the server only when a different revision
  // arrives (initial load, or after our own save/add), so a background refetch
  // never clobbers in-progress edits.
  const loadedRevision = useRef<string | null>(null);
  const loadedSnapshot = useRef<{
    description: string;
    trailing: string;
    sections: string;
  } | null>(null);

  const data = list.data;
  const snapshot = loadedSnapshot.current;
  const dirty = Boolean(
    snapshot &&
    (description !== snapshot.description ||
      trailing !== snapshot.trailing ||
      sectionsSignature(sections) !== snapshot.sections),
  );
  const seedFromServer = useCallback((source: NonNullable<typeof data>) => {
    loadedRevision.current = source.revision;
    loadedSnapshot.current = {
      description: source.description,
      trailing: source.trailing,
      sections: sectionsSignature(serverSections(source.sections)),
    };
    setSections(
      source.sections.map((section, sectionIndex) => ({
        key: `srv-${sectionIndex}`,
        heading: section.heading ?? null,
        marker: section.marker,
        items: section.items.map((item, itemIndex) => ({
          ...item,
          key: `srv-${sectionIndex}-${itemIndex}`,
        })),
      })),
    );
    setDescription(source.description);
    setTrailing(source.trailing);
    setExternalChange(false);
  }, []);
  useEffect(() => {
    if (!data || loadedRevision.current === data.revision) return;
    if (loadedRevision.current !== null && dirty) {
      setExternalChange(true);
      return;
    }
    seedFromServer(data);
  }, [data, dirty, seedFromServer]);

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  // The global mutation-error handler surfaces the message (with a friendly 409
  // notice); preserve the draft and offer an explicit reload on conflict.
  function recoverFromConflict(actionError: unknown) {
    if (isConflictError(actionError)) {
      setExternalChange(true);
      void list.refetch();
    }
  }

  function listPayload(secs: EditableSection[] = sections) {
    return {
      revision: loadedRevision.current ?? data?.revision ?? "",
      description,
      trailing,
      sections: secs.map((section) => ({
        heading: section.heading,
        marker: section.marker,
        items: section.items.map((item) => ({ text: item.text, checked: item.checked })),
      })),
    };
  }

  const save = useMutation({
    mutationFn: () => saveList(id, listPayload()),
    onSuccess: async (detail) => {
      seedFromServer(detail);
      await invalidateLists(id);
    },
    onError: recoverFromConflict,
  });

  // Ticking a todo persists on its own — no trip to the Save button. A short
  // debounce coalesces rapid checks into one write; on success we adopt the new
  // revision and the saved detail in place (no refetch/re-sync), so the toggle
  // stays put and any other in-progress edits aren't clobbered or reset.
  const autoSave = useMutation({
    mutationFn: (payload: ReturnType<typeof listPayload>) => saveList(id, payload),
    onSuccess: (detail) => {
      loadedRevision.current = detail.revision;
      loadedSnapshot.current = {
        description: detail.description,
        trailing: detail.trailing,
        sections: sectionsSignature(serverSections(detail.sections)),
      };
      // Adopt the saved detail in place (no refetch of this list) so a live
      // toggle stays put; only the index needs the fresh counts.
      queryClient.setQueryData(queryKeys.list(id), detail);
      void invalidateLists();
    },
    onError: recoverFromConflict,
  });
  const { schedule: queueAutoSave, cancel: cancelAutoSave } = useDebouncedCallback(
    (payload: ReturnType<typeof listPayload>) => autoSave.mutate(payload),
    500,
  );
  useEffect(() => {
    if (externalChange) cancelAutoSave();
  }, [cancelAutoSave, externalChange]);

  function scheduleAutoSave() {
    queueAutoSave(listPayload(sectionsRef.current));
  }

  // Drop a pending toggle auto-save before any explicit write so a late timer
  // can't fire a redundant save or a stale-revision 409 over it.
  // Adding an entity writes to the file directly (server-side wikilink
  // disambiguation), so persist any pending edits first — otherwise the append
  // would build on the stale on-disk version and the local edits would be lost.
  async function addEntity(entityId: string) {
    cancelAutoSave();
    try {
      if (dirty) await saveList(id, listPayload());
      await addItemToList(id, { entityId });
      await invalidateLists(id);
    } catch (actionError) {
      toast.error(errorMessage(actionError));
      recoverFromConflict(actionError);
    }
  }

  // The section key that owns a draggable id — either a section container id
  // (an empty section is droppable directly) or one of its item keys.
  function containerOf(current: EditableSection[], id: string) {
    if (current.some((section) => section.key === id)) return id;
    return current.find((section) => section.items.some((item) => item.key === id))?.key;
  }

  // While dragging across sections, relocate the active item into the section
  // under the cursor so the move previews live (the dnd-kit multi-container
  // pattern); same-section reordering is finalized in onDragEnd.
  function handleDragOver(event: DragOverEvent) {
    const { active, over } = event;
    if (!over) return;
    setSections((current) => {
      const fromKey = containerOf(current, String(active.id));
      const toKey = containerOf(current, String(over.id));
      if (!fromKey || !toKey || fromKey === toKey) return current;
      const fromSection = current.find((section) => section.key === fromKey)!;
      const moved = fromSection.items.find((item) => item.key === active.id);
      if (!moved) return current;
      const toSection = current.find((section) => section.key === toKey)!;
      const overIndex = toSection.items.findIndex((item) => item.key === over.id);
      const insertAt = overIndex === -1 ? toSection.items.length : overIndex;
      return current.map((section) => {
        if (section.key === fromKey) {
          return { ...section, items: section.items.filter((item) => item.key !== active.id) };
        }
        if (section.key === toKey) {
          const items = [...section.items];
          items.splice(insertAt, 0, moved);
          return { ...section, items };
        }
        return section;
      });
    });
  }

  function handleDragEnd(event: DragEndEvent) {
    const { active, over } = event;
    if (!over || active.id === over.id) return;
    setSections((current) => {
      const key = containerOf(current, String(active.id));
      if (!key || key !== containerOf(current, String(over.id))) return current;
      return current.map((section) => {
        if (section.key !== key) return section;
        const from = section.items.findIndex((item) => item.key === active.id);
        const to = section.items.findIndex((item) => item.key === over.id);
        if (from === -1 || to === -1) return section;
        return { ...section, items: arrayMove(section.items, from, to) };
      });
    });
  }

  function addSection() {
    setSections((current) => [
      ...current,
      { key: nextKey("sec"), heading: t`New section`, marker: "unordered", items: [] },
    ]);
  }

  // Removing a section keeps its items: they fold into the ungrouped block (which
  // is created at the front if the list had none), so nothing is lost.
  function removeSection(key: string) {
    setSections((current) => {
      const target = current.find((section) => section.key === key);
      if (!target) return current;
      const rest = current.filter((section) => section.key !== key);
      if (target.items.length === 0) return rest;
      const ungroupedIndex = rest.findIndex((section) => section.heading === null);
      if (ungroupedIndex === -1) {
        return [
          { key: nextKey("sec"), heading: null, marker: "unordered", items: target.items },
          ...rest,
        ];
      }
      return rest.map((section, index) =>
        index === ungroupedIndex
          ? { ...section, items: [...section.items, ...target.items] }
          : section,
      );
    });
  }

  function updateSection(key: string, patch: Partial<EditableSection>) {
    setSections((current) =>
      current.map((section) => (section.key === key ? { ...section, ...patch } : section)),
    );
  }

  function removeItem(itemKey: string) {
    setSections((current) =>
      current.map((section) => ({
        ...section,
        items: section.items.filter((item) => item.key !== itemKey),
      })),
    );
  }

  function toggleItem(itemKey: string) {
    setSections((current) =>
      current.map((section) => ({
        ...section,
        items: section.items.map((item) =>
          item.key === itemKey ? { ...item, checked: !(item.checked ?? false) } : item,
        ),
      })),
    );
    scheduleAutoSave();
  }

  const rename = useMutation({
    mutationFn: (renameTo: string) => saveList(id, { ...listPayload(), renameTo }),
    onSuccess: async (detail) => {
      setRenameOpen(false);
      await invalidateLists(id);
      navigate(`/lists/${encodeURIComponent(detail.id)}`);
    },
    onError: recoverFromConflict,
  });

  const remove = useMutation({
    mutationFn: () => removeList(id),
    onSuccess: async () => {
      toast.success(t`List deleted`);
      await invalidateLists(id);
      navigate("/lists");
    },
    onError: recoverFromConflict,
  });

  const busy = save.isPending || autoSave.isPending || remove.isPending || rename.isPending;
  const totalItems = sections.reduce((sum, section) => sum + section.items.length, 0);
  const existingIds = new Set(
    sections
      .flatMap((section) => section.items.map((item) => item.entity?.id))
      .filter(Boolean) as string[],
  );

  return (
    <AppFrame error={list.error ? errorMessage(list.error) : undefined}>
      <PageContainer>
        {list.isPending ? (
          <Placeholder>
            <Trans>Loading…</Trans>
          </Placeholder>
        ) : !data ? (
          <Placeholder>
            <Trans>List not found</Trans>
          </Placeholder>
        ) : (
          <>
            <header className="flex flex-wrap items-center gap-2">
              <ListIcon className="size-5 shrink-0 text-muted-foreground" />
              <h1 className="mr-auto truncate text-lg font-semibold">{data.name}</h1>
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={!contentWritable || externalChange || busy}
                onClick={() => setRenameOpen(true)}
              >
                <FilePenLineIcon data-icon="inline-start" />
                <Trans>Rename</Trans>
              </Button>
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={!contentWritable || externalChange || busy}
                onClick={() => setDeleteOpen(true)}
              >
                <Trash2Icon data-icon="inline-start" />
                <Trans>Delete</Trans>
              </Button>
              <Button
                type="button"
                size="sm"
                disabled={!contentWritable || externalChange || !dirty || busy}
                onClick={() => {
                  cancelAutoSave();
                  save.mutate();
                }}
              >
                <SaveIcon data-icon="inline-start" />
                {save.isPending ? <Trans>Saving…</Trans> : <Trans>Save</Trans>}
              </Button>
            </header>

            {externalChange ? (
              <Alert className="flex flex-wrap items-center justify-between gap-3">
                <span className="min-w-0">
                  <Trans comment="Warning banner in the static-list editor after another app changes its Markdown file; the local draft has been preserved">
                    This list changed on disk. Your unsaved edits are still here.
                  </Trans>
                </span>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => {
                    cancelAutoSave();
                    if (data) seedFromServer(data);
                  }}
                  disabled={busy}
                >
                  <Trans>Reload latest version</Trans>
                </Button>
              </Alert>
            ) : null}

            <MarkdownField
              label={t`Description`}
              value={description}
              placeholder={t`Describe this list (appears above the items)…`}
              disabled={!contentWritable || externalChange}
              onChange={setDescription}
            />

            <section className="flex flex-col gap-3">
              <div className="flex items-center gap-2">
                <h2 className="mr-auto text-sm font-medium">
                  <Trans>
                    Items <span className="text-muted-foreground">({totalItems})</span>
                  </Trans>
                </h2>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={!contentWritable || externalChange}
                  onClick={addSection}
                >
                  <FolderPlusIcon data-icon="inline-start" />
                  <Trans>Add section</Trans>
                </Button>
                <Button
                  type="button"
                  size="sm"
                  disabled={!contentWritable || externalChange}
                  onClick={() => setAddOpen(true)}
                >
                  <PlusIcon data-icon="inline-start" />
                  <Trans>Add items</Trans>
                </Button>
              </div>

              {sections.length === 0 ? (
                <div className="rounded-md border border-dashed p-6 text-center text-sm text-muted-foreground">
                  <Trans>
                    No items yet. Use “Add items” to put entities on this list, or “Add section” to
                    group them.
                  </Trans>
                </div>
              ) : (
                <DndContext
                  sensors={sensors}
                  collisionDetection={closestCorners}
                  onDragOver={handleDragOver}
                  onDragEnd={handleDragEnd}
                >
                  <div className="flex flex-col gap-3">
                    {sections.map((section) => (
                      <SectionBlock
                        key={section.key}
                        section={section}
                        language={language}
                        disabled={!contentWritable || externalChange}
                        onHeadingChange={(heading) => updateSection(section.key, { heading })}
                        onMarkerChange={(marker) => updateSection(section.key, { marker })}
                        onRemoveSection={() => removeSection(section.key)}
                        onRemoveItem={removeItem}
                        onToggleItem={toggleItem}
                      />
                    ))}
                  </div>
                </DndContext>
              )}
            </section>

            <MarkdownField
              label={t`Notes`}
              value={trailing}
              placeholder={t`Notes shown below the items…`}
              disabled={!contentWritable || externalChange}
              onChange={setTrailing}
            />

            <AddItemsDialog
              open={addOpen}
              onOpenChange={setAddOpen}
              existingIds={existingIds}
              disabled={!contentWritable || externalChange}
              onAdd={addEntity}
            />
            <RenameDialog
              title={t`Rename list`}
              open={renameOpen}
              onOpenChange={setRenameOpen}
              currentName={data.name}
              saving={rename.isPending}
              disabled={!contentWritable || externalChange}
              onRename={(value) => {
                cancelAutoSave();
                rename.mutate(value);
              }}
            />
            <AlertDialog open={deleteOpen} onOpenChange={setDeleteOpen}>
              <AlertDialogContent>
                <AlertDialogHeader>
                  <AlertDialogTitle>
                    <Trans>Move list to trash?</Trans>
                  </AlertDialogTitle>
                  <AlertDialogDescription>
                    <Trans>
                      This moves <strong>{data.name}</strong> to the Trash. The items it contains
                      are untouched.
                    </Trans>
                  </AlertDialogDescription>
                </AlertDialogHeader>
                <AlertDialogFooter>
                  <AlertDialogCancel disabled={remove.isPending}>
                    <Trans>Cancel</Trans>
                  </AlertDialogCancel>
                  <AlertDialogAction
                    onClick={() => {
                      cancelAutoSave();
                      remove.mutate();
                    }}
                    disabled={remove.isPending}
                  >
                    <Trans>Move to Trash</Trans>
                  </AlertDialogAction>
                </AlertDialogFooter>
              </AlertDialogContent>
            </AlertDialog>
          </>
        )}
      </PageContainer>
    </AppFrame>
  );
}

function MarkdownField({
  label,
  value,
  placeholder,
  disabled,
  onChange,
}: {
  label: string;
  value: string;
  placeholder: string;
  disabled: boolean;
  onChange: (value: string) => void;
}) {
  // Default to the rendered preview; the toggle switches to editing. (Local edit
  // state populates a render after the list loads, so a content-based initial
  // value would latch to edit mode — hence an unconditional default.)
  const [preview, setPreview] = useState(true);

  return (
    <section className="flex flex-col gap-1">
      <div className="flex items-center gap-2">
        <h2 className="mr-auto text-sm font-medium">{label}</h2>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={() => setPreview((current) => !current)}
        >
          {preview ? <PencilIcon data-icon="inline-start" /> : <EyeIcon data-icon="inline-start" />}
          {preview ? <Trans>Edit</Trans> : <Trans>Preview</Trans>}
        </Button>
      </div>
      {preview ? (
        value.trim() ? (
          <MarkdownView markdown={value} relations={[]} />
        ) : null
      ) : (
        <Textarea
          className="min-h-24 font-mono text-code"
          value={value}
          placeholder={placeholder}
          onChange={(event) => onChange(event.target.value)}
          disabled={disabled}
          spellCheck={false}
        />
      )}
    </section>
  );
}
