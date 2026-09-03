//! One section of a list: its heading, its marker style, and its sortable rows.
//!
//! Reordering is drag-and-drop *and* keyboard-driven (dnd-kit's keyboard
//! sensor), so a row is reachable without a pointer.

import { useDroppable } from "@dnd-kit/core";
import { SortableContext, useSortable, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { Trans, useLingui } from "@lingui/react/macro";
import {
  FilePenLineIcon,
  GripVerticalIcon,
  ListIcon,
  ListOrderedIcon,
  MoreHorizontalIcon,
  SquareCheckIcon,
  SquareIcon,
  Trash2Icon,
} from "lucide-react";
import { useState } from "react";
import { Link } from "react-router-dom";

import { EntityCover } from "@/components/assets/entity-cover";
import { EntityTitle } from "@/components/entities/entity-title";
import { Button } from "@/components/ui/button";
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
import { cn } from "@/lib/utils";
import type { ListMarker } from "@/types/api";

import type { EditableItem, EditableSection } from "./list-sections";

export function SectionBlock({
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
  const { t } = useLingui();
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
          <span className="mr-auto px-1 text-xs font-medium tracking-wide text-muted-foreground uppercase">
            <Trans>Ungrouped</Trans>
          </span>
        ) : renaming ? (
          <Input
            autoFocus
            value={section.heading ?? ""}
            placeholder={t`Section heading`}
            disabled={disabled}
            aria-label={t`Section heading`}
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
            {section.heading || t`Untitled section`}
          </h3>
        )}
        <span className="text-xs text-muted-foreground tabular-nums">{section.items.length}</span>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="size-7"
              disabled={disabled}
              aria-label={t`Section actions`}
            >
              <MoreHorizontalIcon />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-44">
            {ungrouped ? null : (
              <>
                <DropdownMenuItem onSelect={() => setRenaming(true)}>
                  <FilePenLineIcon />
                  <Trans>Rename</Trans>
                </DropdownMenuItem>
                <DropdownMenuItem variant="destructive" onSelect={onRemoveSection}>
                  <Trash2Icon />
                  <Trans>Delete</Trans>
                </DropdownMenuItem>
                <DropdownMenuSeparator />
              </>
            )}
            <DropdownMenuLabel>
              <Trans>List style</Trans>
            </DropdownMenuLabel>
            <DropdownMenuRadioGroup
              value={section.marker}
              onValueChange={(value) => onMarkerChange(value as ListMarker)}
            >
              <DropdownMenuRadioItem value="unordered">
                <ListIcon />
                <Trans>Unordered</Trans>
              </DropdownMenuRadioItem>
              <DropdownMenuRadioItem value="ordered">
                <ListOrderedIcon />
                <Trans>Ordered</Trans>
              </DropdownMenuRadioItem>
              <DropdownMenuRadioItem value="todo">
                <SquareCheckIcon />
                <Trans>Todo</Trans>
              </DropdownMenuRadioItem>
            </DropdownMenuRadioGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      <SortableContext
        items={section.items.map((item) => item.key)}
        strategy={verticalListSortingStrategy}
      >
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
            <span className="pointer-events-none">
              <Trans>Drag items here</Trans>
            </span>
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
  const { t } = useLingui();
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: item.key,
  });
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
        aria-label={t`Drag to reorder`}
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
          aria-label={checked ? t`Mark as not done` : t`Mark as done`}
          disabled={disabled}
          onClick={onToggle}
        >
          {checked ? (
            <SquareCheckIcon className="size-4 text-primary" />
          ) : (
            <SquareIcon className="size-4" />
          )}
        </button>
      ) : marker === "ordered" ? (
        <span className="w-5 shrink-0 text-center text-xs text-muted-foreground tabular-nums">
          {index + 1}.
        </span>
      ) : null}
      {item.entity ? (
        <Link
          to={`/entities/${encodeURIComponent(item.entity.id)}`}
          className="flex min-w-0 flex-1 items-center gap-2 rounded hover:bg-accent/40"
        >
          <EntityCover entity={item.entity} />
          <span className="min-w-0">
            <EntityTitle
              as="span"
              entity={item.entity}
              language={language}
              className={cn(
                "block truncate text-sm font-medium",
                marker === "todo" && checked && "line-through",
              )}
            />
            <span className="block truncate text-xs text-muted-foreground">
              {item.entity.typeLabel}
            </span>
          </span>
        </Link>
      ) : (
        <span className="flex min-w-0 flex-1 flex-col">
          <span className={cn("truncate text-sm", marker === "todo" && checked && "line-through")}>
            {item.text}
          </span>
          <span className="text-xs text-muted-foreground">
            <Trans>Unresolved link</Trans>
          </span>
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
        aria-label={t`Remove item`}
      >
        <Trash2Icon />
      </Button>
    </li>
  );
}
