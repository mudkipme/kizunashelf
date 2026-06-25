import { useEffect, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  DndContext,
  KeyboardSensor,
  PointerSensor,
  closestCorners,
  useDroppable,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragOverEvent,
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
  FolderPlusIcon,
  GripVerticalIcon,
  ListIcon,
  ListOrderedIcon,
  MoreHorizontalIcon,
  PencilIcon,
  PlusIcon,
  SaveIcon,
  SquareCheckIcon,
  SquareIcon,
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
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { basenameValidationError, normalizeBasename } from "@/lib/basename";
import { useTitleLanguage } from "@/lib/language";
import { entityTitle } from "@/lib/title-language";
import { cn } from "@/lib/utils";
import type { ListItem, ListMarker, ListSection } from "@/types/api";

type EditableItem = ListItem & { key: string };
type EditableSection = {
  key: string;
  heading: string | null;
  marker: ListMarker;
  items: EditableItem[];
};

// A stable-ish signature of the editable sections, for dirty-tracking and for
// comparing local edits against the server's last-loaded state. Only the parts
// that round-trip to Markdown matter (heading, marker, item order + task state).
function sectionsSignature(
  sections: Array<{ heading: string | null; marker: ListMarker; items: Array<{ text: string; checked?: boolean | null }> }>,
) {
  return JSON.stringify(
    sections.map((section) => ({
      heading: section.heading,
      marker: section.marker,
      items: section.items.map((item) => ({ text: item.text, checked: item.checked ?? null })),
    })),
  );
}

function serverSections(sections: ListSection[]) {
  return sections.map((section) => ({
    heading: section.heading ?? null,
    marker: section.marker,
    items: section.items,
  }));
}

export function ListDetailPage() {
  const { id = "" } = useParams();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const language = useTitleLanguage();

  const list = useQuery({ ...listQuery(id), enabled: Boolean(id) });
  const capabilities = useQuery(capabilitiesQuery());
  const contentWritable = capabilities.data?.contentWritable !== false;

  const [sections, setSections] = useState<EditableSection[]>([]);
  const [description, setDescription] = useState("");
  const [trailing, setTrailing] = useState("");
  const [error, setError] = useState<string>();
  const [addOpen, setAddOpen] = useState(false);
  const [renameOpen, setRenameOpen] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);
  // Monotonic counter for fresh React keys on locally-created sections/items, so
  // dnd-kit identities stay stable across edits without colliding with `srv-*`.
  const keyCounter = useRef(0);
  const nextKey = (prefix: string) => `${prefix}-${(keyCounter.current += 1)}`;
  // Re-sync local edit state from the server only when a different revision
  // arrives (initial load, or after our own save/add), so a background refetch
  // never clobbers in-progress edits.
  const loadedRevision = useRef<string | null>(null);

  const data = list.data;
  useEffect(() => {
    if (!data || loadedRevision.current === data.revision) return;
    loadedRevision.current = data.revision;
    setSections(
      data.sections.map((section, sectionIndex) => ({
        key: `srv-${sectionIndex}`,
        heading: section.heading ?? null,
        marker: section.marker,
        items: section.items.map((item, itemIndex) => ({ ...item, key: `srv-${sectionIndex}-${itemIndex}` })),
      })),
    );
    setDescription(data.description);
    setTrailing(data.trailing);
  }, [data]);

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  const dirty = Boolean(
    data &&
      (description !== data.description ||
        trailing !== data.trailing ||
        sectionsSignature(sections) !== sectionsSignature(serverSections(data.sections))),
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
      sections: sections.map((section) => ({
        heading: section.heading,
        marker: section.marker,
        items: section.items.map((item) => ({ text: item.text, checked: item.checked })),
      })),
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
      { key: nextKey("sec"), heading: "New section", marker: "unordered", items: [] },
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
        return [{ key: nextKey("sec"), heading: null, marker: "unordered", items: target.items }, ...rest];
      }
      return rest.map((section, index) =>
        index === ungroupedIndex ? { ...section, items: [...section.items, ...target.items] } : section,
      );
    });
  }

  function updateSection(key: string, patch: Partial<EditableSection>) {
    setSections((current) => current.map((section) => (section.key === key ? { ...section, ...patch } : section)));
  }

  function removeItem(itemKey: string) {
    setSections((current) =>
      current.map((section) => ({ ...section, items: section.items.filter((item) => item.key !== itemKey) })),
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
  const totalItems = sections.reduce((sum, section) => sum + section.items.length, 0);
  const existingIds = new Set(
    sections.flatMap((section) => section.items.map((item) => item.entity?.id)).filter(Boolean) as string[],
  );

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

            <section className="flex flex-col gap-3">
              <div className="flex items-center gap-2">
                <h2 className="mr-auto text-sm font-medium">
                  Items <span className="text-muted-foreground">({totalItems})</span>
                </h2>
                <Button type="button" variant="outline" size="sm" disabled={!contentWritable} onClick={addSection}>
                  <FolderPlusIcon data-icon="inline-start" />
                  Add section
                </Button>
                <Button type="button" size="sm" disabled={!contentWritable} onClick={() => setAddOpen(true)}>
                  <PlusIcon data-icon="inline-start" />
                  Add items
                </Button>
              </div>

              {sections.length === 0 ? (
                <div className="rounded-md border border-dashed p-6 text-center text-sm text-muted-foreground">
                  No items yet. Use “Add items” to put entities on this list, or “Add section” to group them.
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
                        disabled={!contentWritable}
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

function SectionBlock({
  section,
  language,
  disabled,
  onHeadingChange,
  onMarkerChange,
  onRemoveSection,
  onRemoveItem,
  onToggleItem,
}: {
  section: EditableSection;
  language: string;
  disabled: boolean;
  onHeadingChange: (heading: string) => void;
  onMarkerChange: (marker: ListMarker) => void;
  onRemoveSection: () => void;
  onRemoveItem: (itemKey: string) => void;
  onToggleItem: (itemKey: string) => void;
}) {
  // Each section is a drop target in its own right, so items can be dragged into
  // an empty one (where there are no item rows to drop onto).
  const { setNodeRef, isOver } = useDroppable({ id: section.key });
  const ungrouped = section.heading === null;
  // The heading reads as plain text until the user picks "Rename" from the menu.
  const [renaming, setRenaming] = useState(false);

  return (
    <div className="rounded-md border bg-muted/30 p-2">
      <div className="mb-2 flex items-center gap-2">
        {ungrouped ? (
          <span className="mr-auto px-1 text-xs font-medium uppercase tracking-wide text-muted-foreground">
            Ungrouped
          </span>
        ) : renaming ? (
          <Input
            autoFocus
            value={section.heading ?? ""}
            placeholder="Section heading"
            disabled={disabled}
            aria-label="Section heading"
            className="mr-auto h-7 max-w-xs text-sm font-medium"
            onChange={(event) => onHeadingChange(event.target.value)}
            onBlur={() => setRenaming(false)}
            onKeyDown={(event) => {
              if (event.key === "Enter" || event.key === "Escape") {
                event.preventDefault();
                setRenaming(false);
              }
            }}
          />
        ) : (
          <h3 className="mr-auto truncate px-1 text-sm font-semibold">
            {section.heading || "Untitled section"}
          </h3>
        )}
        <span className="text-xs tabular-nums text-muted-foreground">{section.items.length}</span>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="size-7"
              disabled={disabled}
              aria-label="Section actions"
            >
              <MoreHorizontalIcon />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-44">
            {ungrouped ? null : (
              <>
                <DropdownMenuItem onSelect={() => setRenaming(true)}>
                  <FilePenLineIcon />
                  Rename
                </DropdownMenuItem>
                <DropdownMenuItem variant="destructive" onSelect={onRemoveSection}>
                  <Trash2Icon />
                  Delete
                </DropdownMenuItem>
                <DropdownMenuSeparator />
              </>
            )}
            <DropdownMenuLabel>List style</DropdownMenuLabel>
            <DropdownMenuRadioGroup
              value={section.marker}
              onValueChange={(value) => onMarkerChange(value as ListMarker)}
            >
              <DropdownMenuRadioItem value="unordered">
                <ListIcon />
                Unordered
              </DropdownMenuRadioItem>
              <DropdownMenuRadioItem value="ordered">
                <ListOrderedIcon />
                Ordered
              </DropdownMenuRadioItem>
              <DropdownMenuRadioItem value="todo">
                <SquareCheckIcon />
                Todo
              </DropdownMenuRadioItem>
            </DropdownMenuRadioGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      <SortableContext items={section.items.map((item) => item.key)} strategy={verticalListSortingStrategy}>
        <ol
          ref={setNodeRef}
          className={cn(
            "flex min-h-10 flex-col gap-2 rounded-md transition-colors",
            isOver && "bg-accent/40",
            section.items.length === 0 &&
              "items-center justify-center border border-dashed p-3 text-center text-xs text-muted-foreground",
          )}
        >
          {section.items.length === 0 ? (
            <span className="pointer-events-none">Drag items here</span>
          ) : (
            section.items.map((item, index) => (
              <SortableRow
                key={item.key}
                item={item}
                index={index}
                marker={section.marker}
                language={language}
                disabled={disabled}
                onRemove={() => onRemoveItem(item.key)}
                onToggle={() => onToggleItem(item.key)}
              />
            ))
          )}
        </ol>
      </SortableContext>
    </div>
  );
}

function SortableRow({
  item,
  index,
  marker,
  language,
  disabled,
  onRemove,
  onToggle,
}: {
  item: EditableItem;
  index: number;
  marker: ListMarker;
  language: string;
  disabled: boolean;
  onRemove: () => void;
  onToggle: () => void;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id: item.key });
  const style = { transform: CSS.Transform.toString(transform), transition };
  const checked = item.checked ?? false;

  return (
    <li
      ref={setNodeRef}
      style={style}
      className={cn(
        "group flex items-center gap-2 rounded-md border bg-card p-2",
        isDragging && "opacity-60 shadow-sm",
        // A checked-off task reads as "done": dimmed and struck through.
        marker === "todo" && checked && "opacity-60",
      )}
    >
      <button
        type="button"
        className={cn(
          "flex size-7 shrink-0 cursor-grab touch-none items-center justify-center rounded text-muted-foreground transition-opacity hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50",
          // Keep rows reading as content; the grip surfaces on hover/focus.
          "opacity-0 group-focus-within:opacity-100 group-hover:opacity-100",
          disabled && "hidden",
        )}
        aria-label="Drag to reorder"
        disabled={disabled}
        {...attributes}
        {...listeners}
      >
        <GripVerticalIcon className="size-4" />
      </button>
      {/* Marker column: a checkbox for todo, the position for ordered, and nothing
          for unordered (the card itself already separates rows). */}
      {marker === "todo" ? (
        <button
          type="button"
          className="flex size-5 shrink-0 items-center justify-center rounded text-muted-foreground hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
          role="checkbox"
          aria-checked={checked}
          aria-label={checked ? "Mark as not done" : "Mark as done"}
          disabled={disabled}
          onClick={onToggle}
        >
          {checked ? <SquareCheckIcon className="size-4 text-primary" /> : <SquareIcon className="size-4" />}
        </button>
      ) : marker === "ordered" ? (
        <span className="w-5 shrink-0 text-center text-xs tabular-nums text-muted-foreground">{index + 1}.</span>
      ) : null}
      {item.entity ? (
        <Link
          to={`/entities/${encodeURIComponent(item.entity.id)}`}
          className="flex min-w-0 flex-1 items-center gap-2 rounded hover:bg-accent/40"
        >
          <EntityCover entity={item.entity} />
          <span className="min-w-0">
            <span className={cn("block truncate text-sm font-medium", marker === "todo" && checked && "line-through")}>
              {entityTitle(item.entity, language)}
            </span>
            <span className="block truncate text-xs text-muted-foreground">{item.entity.typeLabel}</span>
          </span>
        </Link>
      ) : (
        <span className="flex min-w-0 flex-1 flex-col">
          <span className={cn("truncate text-sm", marker === "todo" && checked && "line-through")}>{item.text}</span>
          <span className="text-xs text-muted-foreground">Unresolved link</span>
        </span>
      )}
      <Button
        type="button"
        variant="ghost"
        size="icon"
        className={cn(
          "shrink-0 text-muted-foreground transition-opacity",
          "opacity-0 group-focus-within:opacity-100 group-hover:opacity-100",
          disabled && "hidden",
        )}
        onClick={onRemove}
        disabled={disabled}
        aria-label="Remove item"
      >
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
