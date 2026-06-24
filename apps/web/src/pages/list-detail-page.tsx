import { useEffect, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  DndContext,
  KeyboardSensor,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  arrayMove,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import {
  CheckIcon,
  EyeIcon,
  FilePenLineIcon,
  GripVerticalIcon,
  ListIcon,
  ListOrderedIcon,
  PencilIcon,
  PlusIcon,
  SaveIcon,
  Trash2Icon,
  XIcon,
} from "lucide-react";
import { Link, useNavigate, useParams } from "react-router-dom";

import { errorMessage, isConflictError } from "@/api/client";
import { addItemToList, removeList, saveList } from "@/api/lists";
import {
  capabilitiesQuery,
  entitiesQuery,
  listQuery,
  queryKeys,
} from "@/api/queries";
import { EntityCover } from "@/components/assets/entity-cover";
import { MarkdownView } from "@/components/assets/markdown-view";
import { AppFrame } from "@/components/layout/app-frame";
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
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { basenameValidationError, normalizeBasename } from "@/lib/basename";
import { useTitleLanguage } from "@/lib/language";
import { entityTitle } from "@/lib/title-language";
import { cn } from "@/lib/utils";
import type { ListItem } from "@/types/api";

type EditableItem = ListItem & { key: string };

export function ListDetailPage() {
  const { id = "" } = useParams();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const language = useTitleLanguage();

  const list = useQuery({ ...listQuery(id), enabled: Boolean(id) });
  const capabilities = useQuery(capabilitiesQuery());
  const contentWritable = capabilities.data?.contentWritable !== false;

  const [items, setItems] = useState<EditableItem[]>([]);
  const [description, setDescription] = useState("");
  const [trailing, setTrailing] = useState("");
  const [ordered, setOrdered] = useState(false);
  const [error, setError] = useState<string>();
  const [addOpen, setAddOpen] = useState(false);
  const [renameOpen, setRenameOpen] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);
  // Re-sync local edit state from the server only when a different revision
  // arrives (initial load, or after our own save/add), so a background refetch
  // never clobbers in-progress edits.
  const loadedRevision = useRef<string | null>(null);

  const data = list.data;
  useEffect(() => {
    if (!data || loadedRevision.current === data.revision) return;
    loadedRevision.current = data.revision;
    setItems(data.items.map((item, index) => ({ ...item, key: `srv-${index}` })));
    setDescription(data.description);
    setTrailing(data.trailing);
    setOrdered(data.ordered);
  }, [data]);

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  const dirty = Boolean(
    data &&
      (description !== data.description ||
        trailing !== data.trailing ||
        ordered !== data.ordered ||
        items.length !== data.items.length ||
        items.some((item, index) => item.text !== data.items[index]?.text)),
  );

  function invalidate() {
    return Promise.all([
      queryClient.invalidateQueries({ queryKey: queryKeys.list(id) }),
      queryClient.invalidateQueries({ queryKey: queryKeys.lists }),
    ]);
  }

  function reportError(actionError: unknown) {
    if (isConflictError(actionError)) {
      setError("This list changed on disk since it was loaded. Reloaded the latest version — please try again.");
      void list.refetch();
    } else {
      setError(errorMessage(actionError));
    }
  }

  function listPayload() {
    return {
      revision: data?.revision ?? "",
      description,
      trailing,
      ordered,
      items: items.map((item) => ({ text: item.text })),
    };
  }

  const save = useMutation({
    mutationFn: () => saveList(id, listPayload()),
    onSuccess: async () => {
      setError(undefined);
      await invalidate();
    },
    onError: reportError,
  });

  // Adding an entity writes to the file directly (server-side wikilink
  // disambiguation), so persist any pending edits first — otherwise the append
  // would build on the stale on-disk version and the local edits would be lost.
  async function addEntity(entityId: string) {
    setError(undefined);
    try {
      if (dirty) await saveList(id, listPayload());
      await addItemToList(id, { entityId });
      await invalidate();
    } catch (actionError) {
      reportError(actionError);
    }
  }

  function handleDragEnd(event: DragEndEvent) {
    const { active, over } = event;
    if (!over || active.id === over.id) return;
    setItems((current) => {
      const from = current.findIndex((item) => item.key === active.id);
      const to = current.findIndex((item) => item.key === over.id);
      if (from === -1 || to === -1) return current;
      return arrayMove(current, from, to);
    });
  }

  const rename = useMutation({
    mutationFn: (renameTo: string) => saveList(id, { ...listPayload(), renameTo }),
    onSuccess: async (detail) => {
      setError(undefined);
      setRenameOpen(false);
      await invalidate();
      navigate(`/lists/${encodeURIComponent(detail.id)}`);
    },
    onError: reportError,
  });

  const remove = useMutation({
    mutationFn: () => removeList(id),
    onSuccess: async () => {
      await invalidate();
      navigate("/lists");
    },
    onError: reportError,
  });

  const busy = save.isPending || remove.isPending || rename.isPending;
  const existingIds = new Set(items.map((item) => item.entity?.id).filter(Boolean) as string[]);

  return (
    <AppFrame error={error ?? (list.error ? errorMessage(list.error) : undefined)}>
      <div className="mx-auto flex w-full max-w-3xl flex-col gap-4 p-4">
        {list.isPending ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">Loading</div>
        ) : !data ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">List not found</div>
        ) : (
          <>
            <header className="flex flex-wrap items-center gap-2">
              <ListIcon className="size-5 shrink-0 text-muted-foreground" />
              <h1 className="mr-auto truncate text-lg font-semibold">{data.name}</h1>
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={!contentWritable || busy}
                onClick={() => setRenameOpen(true)}
              >
                <FilePenLineIcon data-icon="inline-start" />
                Rename
              </Button>
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={!contentWritable || busy}
                onClick={() => setDeleteOpen(true)}
              >
                <Trash2Icon data-icon="inline-start" />
                Delete
              </Button>
              <Button type="button" size="sm" disabled={!contentWritable || !dirty || busy} onClick={() => save.mutate()}>
                <SaveIcon data-icon="inline-start" />
                {save.isPending ? "Saving" : "Save"}
              </Button>
            </header>

            <MarkdownField
              label="Description"
              value={description}
              placeholder="Describe this list (appears above the items)…"
              disabled={!contentWritable}
              onChange={setDescription}
            />

            <section className="flex flex-col gap-2">
              <div className="flex items-center gap-2">
                <h2 className="mr-auto text-sm font-medium">
                  Items <span className="text-muted-foreground">({items.length})</span>
                </h2>
                <div className="flex overflow-hidden rounded-md border">
                  <button
                    type="button"
                    className={cn(
                      "flex h-8 items-center gap-1 px-2 text-xs transition-colors",
                      !ordered ? "bg-accent text-foreground" : "text-muted-foreground hover:bg-accent/50",
                    )}
                    disabled={!contentWritable}
                    aria-pressed={!ordered}
                    onClick={() => setOrdered(false)}
                  >
                    <ListIcon className="size-3.5" />
                    Bulleted
                  </button>
                  <button
                    type="button"
                    className={cn(
                      "flex h-8 items-center gap-1 border-l px-2 text-xs transition-colors",
                      ordered ? "bg-accent text-foreground" : "text-muted-foreground hover:bg-accent/50",
                    )}
                    disabled={!contentWritable}
                    aria-pressed={ordered}
                    onClick={() => setOrdered(true)}
                  >
                    <ListOrderedIcon className="size-3.5" />
                    Numbered
                  </button>
                </div>
                <Button type="button" size="sm" disabled={!contentWritable} onClick={() => setAddOpen(true)}>
                  <PlusIcon data-icon="inline-start" />
                  Add items
                </Button>
              </div>

              {items.length === 0 ? (
                <div className="rounded-md border border-dashed p-6 text-center text-sm text-muted-foreground">
                  No items yet. Use “Add items” to put entities on this list.
                </div>
              ) : (
                <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
                  <SortableContext items={items.map((item) => item.key)} strategy={verticalListSortingStrategy}>
                    <ol className="flex flex-col gap-2">
                      {items.map((item, index) => (
                        <SortableRow
                          key={item.key}
                          item={item}
                          index={index}
                          ordered={ordered}
                          language={language}
                          disabled={!contentWritable}
                          onRemove={() => setItems((current) => current.filter((entry) => entry.key !== item.key))}
                        />
                      ))}
                    </ol>
                  </SortableContext>
                </DndContext>
              )}
            </section>

            <MarkdownField
              label="Notes"
              value={trailing}
              placeholder="Notes shown below the items…"
              disabled={!contentWritable}
              onChange={setTrailing}
            />

            <AddItemsDialog
              open={addOpen}
              onOpenChange={setAddOpen}
              existingIds={existingIds}
              disabled={!contentWritable}
              onAdd={addEntity}
            />
            <RenameListDialog
              open={renameOpen}
              onOpenChange={setRenameOpen}
              currentName={data.name}
              saving={rename.isPending}
              disabled={!contentWritable}
              onRename={(value) => rename.mutate(value)}
            />
            <AlertDialog open={deleteOpen} onOpenChange={setDeleteOpen}>
              <AlertDialogContent>
                <AlertDialogHeader>
                  <AlertDialogTitle>Move list to trash?</AlertDialogTitle>
                  <AlertDialogDescription>
                    This moves <strong>{data.name}</strong> to the Trash. The items it contains are
                    untouched.
                  </AlertDialogDescription>
                </AlertDialogHeader>
                <AlertDialogFooter>
                  <AlertDialogCancel disabled={remove.isPending}>Cancel</AlertDialogCancel>
                  <AlertDialogAction onClick={() => remove.mutate()} disabled={remove.isPending}>
                    Move to Trash
                  </AlertDialogAction>
                </AlertDialogFooter>
              </AlertDialogContent>
            </AlertDialog>
          </>
        )}
      </div>
    </AppFrame>
  );
}

function SortableRow({
  item,
  index,
  ordered,
  language,
  disabled,
  onRemove,
}: {
  item: EditableItem;
  index: number;
  ordered: boolean;
  language: string;
  disabled: boolean;
  onRemove: () => void;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id: item.key });
  const style = { transform: CSS.Transform.toString(transform), transition };

  return (
    <li
      ref={setNodeRef}
      style={style}
      className={cn(
        "flex items-center gap-2 rounded-md border bg-card p-2",
        isDragging && "opacity-60 shadow-sm",
      )}
    >
      <button
        type="button"
        className="flex size-7 shrink-0 cursor-grab touch-none items-center justify-center rounded text-muted-foreground hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
        aria-label="Drag to reorder"
        disabled={disabled}
        {...attributes}
        {...listeners}
      >
        <GripVerticalIcon className="size-4" />
      </button>
      <span className="w-5 shrink-0 text-center text-xs tabular-nums text-muted-foreground">
        {ordered ? `${index + 1}.` : "•"}
      </span>
      {item.entity ? (
        <Link
          to={`/entities/${encodeURIComponent(item.entity.id)}`}
          className="flex min-w-0 flex-1 items-center gap-2 rounded hover:bg-accent/40"
        >
          <EntityCover entity={item.entity} />
          <span className="min-w-0">
            <span className="block truncate text-sm font-medium">{entityTitle(item.entity, language)}</span>
            <span className="block truncate text-xs text-muted-foreground">{item.entity.typeLabel}</span>
          </span>
        </Link>
      ) : (
        <span className="flex min-w-0 flex-1 flex-col">
          <span className="truncate text-sm">{item.text}</span>
          <span className="text-xs text-muted-foreground">Unresolved link</span>
        </span>
      )}
      <Button type="button" variant="ghost" size="icon" onClick={onRemove} disabled={disabled} aria-label="Remove item">
        <Trash2Icon />
      </Button>
    </li>
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
        <Button type="button" variant="ghost" size="sm" onClick={() => setPreview((current) => !current)}>
          {preview ? <PencilIcon data-icon="inline-start" /> : <EyeIcon data-icon="inline-start" />}
          {preview ? "Edit" : "Preview"}
        </Button>
      </div>
      {preview ? (
        value.trim() ? (
          <div className="rounded-md border p-3">
            <MarkdownView markdown={value} relations={[]} />
          </div>
        ) : (
          <div className="rounded-md border border-dashed p-3 text-sm text-muted-foreground">Nothing to preview.</div>
        )
      ) : (
        <Textarea
          className="min-h-24 font-mono text-xs"
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

function AddItemsDialog({
  open,
  onOpenChange,
  existingIds,
  disabled,
  onAdd,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  existingIds: Set<string>;
  disabled: boolean;
  onAdd: (entityId: string) => Promise<void>;
}) {
  const language = useTitleLanguage();
  const [query, setQuery] = useState("");
  const [pendingId, setPendingId] = useState<string>();
  const search = useQuery({
    ...entitiesQuery({ q: query.trim() || undefined, pageSize: 20, titleLanguage: language }),
    enabled: open,
  });
  const results = search.data?.items ?? [];

  async function add(entityId: string) {
    setPendingId(entityId);
    try {
      await onAdd(entityId);
    } finally {
      setPendingId(undefined);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Add items</DialogTitle>
          <DialogDescription>Search entities and add them to this list.</DialogDescription>
        </DialogHeader>
        <Input
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="Search entities…"
          autoFocus
        />
        <div className="flex max-h-80 flex-col gap-1 overflow-auto">
          {search.isPending ? (
            <p className="p-3 text-center text-sm text-muted-foreground">Loading</p>
          ) : results.length === 0 ? (
            <p className="p-3 text-center text-sm text-muted-foreground">No matching entities.</p>
          ) : (
            results.map((entity) => {
              const added = existingIds.has(entity.id);
              return (
                <div key={entity.id} className="flex items-center gap-2 rounded-md p-1">
                  <EntityCover entity={entity} />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-sm font-medium">{entityTitle(entity, language)}</span>
                    <span className="block truncate text-xs text-muted-foreground">{entity.typeLabel}</span>
                  </span>
                  {added ? (
                    <Badge variant="outline">Added</Badge>
                  ) : (
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      disabled={disabled || pendingId === entity.id}
                      onClick={() => void add(entity.id)}
                    >
                      <PlusIcon data-icon="inline-start" />
                      Add
                    </Button>
                  )}
                </div>
              );
            })
          )}
        </div>
        <DialogFooter>
          <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
            <CheckIcon data-icon="inline-start" />
            Done
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function RenameListDialog({
  open,
  onOpenChange,
  currentName,
  saving,
  disabled,
  onRename,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  currentName: string;
  saving: boolean;
  disabled: boolean;
  onRename: (name: string) => void;
}) {
  const [name, setName] = useState(currentName);
  useEffect(() => {
    if (open) setName(currentName);
  }, [open, currentName]);

  const normalized = normalizeBasename(name);
  const validationError = name.trim() ? basenameValidationError(normalized) : undefined;
  const unchanged = normalized === currentName;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Rename list</DialogTitle>
          <DialogDescription>Changes the list's name.</DialogDescription>
        </DialogHeader>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            if (!name.trim() || validationError || unchanged) return;
            onRename(normalized);
          }}
          className="flex flex-col gap-2"
        >
          <label className="text-sm font-medium">
            Name
            <Input
              value={name}
              onChange={(event) => setName(event.target.value)}
              disabled={disabled || saving}
              aria-invalid={Boolean(validationError)}
            />
          </label>
          {validationError ? <p className="text-xs text-destructive">{validationError}</p> : null}
          <DialogFooter className="mt-2">
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)} disabled={saving}>
              <XIcon data-icon="inline-start" />
              Cancel
            </Button>
            <Button type="submit" disabled={disabled || saving || Boolean(validationError) || unchanged || !name.trim()}>
              <CheckIcon data-icon="inline-start" />
              {saving ? "Renaming" : "Rename"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
