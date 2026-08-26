import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { userEvent } from "vitest/browser";
import {
  DndContext,
  KeyboardSensor,
  PointerSensor,
  closestCorners,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import { sortableKeyboardCoordinates } from "@dnd-kit/sortable";

import { SectionBlock } from "@/components/lists/section-block";
import type { EditableItem, EditableSection } from "@/components/lists/list-sections";
import { render } from "@/test/render";
import type { EntitySummary } from "@/types/api";

function entity(overrides: Partial<EntitySummary> & Pick<EntitySummary, "id">): EntitySummary {
  return {
    type: "anime",
    typeLabel: "Anime",
    title: overrides.id,
    titles: {},
    dates: [],
    image: null,
    path: `${overrides.id}.md`,
    basename: overrides.id,
    externalRefs: {},
    relationCount: 0,
    ...overrides,
  };
}

function item(text: string, overrides: Partial<EditableItem> = {}): EditableItem {
  return { key: text, text, ...overrides };
}

function section(overrides: Partial<EditableSection> = {}): EditableSection {
  return { key: "s1", heading: "Watching", marker: "unordered", items: [], ...overrides };
}

/**
 * A section only makes sense inside the page's drag context — `useSortable` and
 * `useDroppable` both need one — so the harness sets up the same sensors the
 * list page does, including the keyboard sensor that makes reordering reachable
 * without a pointer.
 */
function Harness({
  initial,
  onDragEnd,
  ...handlers
}: {
  initial: EditableSection;
  onDragEnd?: (event: DragEndEvent) => void;
  disabled?: boolean;
  onHeadingChange?: (heading: string) => void;
  onMarkerChange?: (marker: EditableSection["marker"]) => void;
  onRemoveSection?: () => void;
  onRemoveItem?: (key: string) => void;
  onToggleItem?: (key: string) => void;
}) {
  const [current, setCurrent] = useState(initial);
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );
  return (
    <DndContext sensors={sensors} collisionDetection={closestCorners} onDragEnd={onDragEnd}>
      <SectionBlock
        section={current}
        language="en"
        disabled={handlers.disabled ?? false}
        onHeadingChange={(heading) => {
          setCurrent((prev) => ({ ...prev, heading }));
          handlers.onHeadingChange?.(heading);
        }}
        onMarkerChange={(marker) => {
          setCurrent((prev) => ({ ...prev, marker }));
          handlers.onMarkerChange?.(marker);
        }}
        onRemoveSection={() => handlers.onRemoveSection?.()}
        onRemoveItem={(key) => handlers.onRemoveItem?.(key)}
        onToggleItem={(key) => {
          setCurrent((prev) => ({
            ...prev,
            items: prev.items.map((row) =>
              row.key === key ? { ...row, checked: !(row.checked ?? false) } : row,
            ),
          }));
          handlers.onToggleItem?.(key);
        }}
      />
    </DndContext>
  );
}

describe("SectionBlock", () => {
  it("offers no rename or delete for the ungrouped block", async () => {
    const screen = await render(
      <Harness initial={section({ heading: null, items: [item("[[Frieren]]")] })} />,
    );

    await expect.element(screen.getByText("Ungrouped")).toBeVisible();
    await screen.getByRole("button", { name: "Section actions" }).click();

    // The ungrouped block is not a `## heading` in the Markdown — it is the
    // items above the first one — so there is nothing to rename or delete.
    expect(screen.getByRole("menuitem").elements().map((entry) => entry.textContent)).toEqual([]);
    await expect.element(screen.getByRole("menuitemradio", { name: "Unordered" })).toBeVisible();
  });

  it("renames a section in place, and stops on Enter", async () => {
    const onHeadingChange = vi.fn();
    const screen = await render(
      <Harness initial={section({ heading: "Watching" })} onHeadingChange={onHeadingChange} />,
    );

    // A heading reads as content until it is explicitly being renamed.
    await expect.element(screen.getByRole("heading", { name: "Watching" })).toBeVisible();

    await screen.getByRole("button", { name: "Section actions" }).click();
    await screen.getByRole("menuitem", { name: "Rename" }).click();

    const input = screen.getByRole("textbox", { name: "Section heading" });
    await input.fill("On hold");
    expect(onHeadingChange).toHaveBeenLastCalledWith("On hold");

    await userEvent.keyboard("{Enter}");
    await expect.element(screen.getByRole("heading", { name: "On hold" })).toBeVisible();
  });

  it("changes the list style from the section menu", async () => {
    const onMarkerChange = vi.fn();
    const screen = await render(
      <Harness initial={section({ items: [item("[[Frieren]]")] })} onMarkerChange={onMarkerChange} />,
    );

    await screen.getByRole("button", { name: "Section actions" }).click();
    await screen.getByRole("menuitemradio", { name: "Ordered", exact: true }).click();

    expect(onMarkerChange).toHaveBeenLastCalledWith("ordered");
    // The marker is a Markdown fact (`-` vs `1.` vs `- [ ]`), so the rows
    // restyle to match what will be written.
    await expect.element(screen.getByText("1.")).toBeVisible();
  });

  it("marks rows the way the section's marker says, and nothing more", async () => {
    const screen = await render(
      <Harness initial={section({ marker: "unordered", items: [item("a"), item("b")] })} />,
    );

    // An unordered section has no per-row marker at all: the row cards already
    // separate the items, and a bullet would just be noise.
    expect(screen.getByRole("checkbox").elements()).toHaveLength(0);
    expect(screen.getByText("1.").elements()).toHaveLength(0);
  });

  it("ticks a task off without leaving the list", async () => {
    const onToggleItem = vi.fn();
    const screen = await render(
      <Harness
        initial={section({ marker: "todo", items: [item("Watch ep 1"), item("Watch ep 2", { checked: true })] })}
        onToggleItem={onToggleItem}
      />,
    );

    const first = screen.getByRole("checkbox", { name: "Mark as done" });
    await expect.element(screen.getByRole("checkbox", { name: "Mark as not done" })).toBeVisible();

    await first.click();

    expect(onToggleItem).toHaveBeenLastCalledWith("Watch ep 1");
    // The label flips with the state, so the control says what it will do next
    // rather than what it currently is.
    expect(screen.getByRole("checkbox", { name: "Mark as not done" }).elements()).toHaveLength(2);
  });

  it("links a resolved item to its entity and shows an unresolved one as its raw text", async () => {
    const screen = await render(
      <Harness
        initial={section({
          items: [
            item("[[Frieren]] — rewatch", { entity: entity({ id: "anime/Frieren", title: "Frieren" }) }),
            // A wikilink to a note the index does not know about: kept
            // verbatim, since the file is the source of truth and the link may
            // simply not exist yet.
            item("[[Not indexed]]"),
          ],
        })}
      />,
    );

    await expect
      .element(screen.getByRole("link", { name: /Frieren/ }))
      .toHaveAttribute("href", "/entities/anime%2FFrieren");
    await expect.element(screen.getByText("[[Not indexed]]")).toBeVisible();
    await expect.element(screen.getByText("Unresolved link")).toBeVisible();
  });

  it("removes an item by its stable key, not its position", async () => {
    const onRemoveItem = vi.fn();
    const screen = await render(
      <Harness
        initial={section({ items: [item("first"), item("second")] })}
        onRemoveItem={onRemoveItem}
      />,
    );

    await screen.getByRole("button", { name: "Remove item" }).nth(1).click();
    expect(onRemoveItem).toHaveBeenLastCalledWith("second");
  });

  it("invites a drop when the section is empty", async () => {
    const screen = await render(<Harness initial={section({ items: [] })} />);

    // An empty section is still a drop target — otherwise a freshly added
    // section could never receive its first item.
    await expect.element(screen.getByText("Drag items here")).toBeVisible();
  });

  it("reorders with the keyboard alone", async () => {
    const onDragEnd = vi.fn();
    const screen = await render(
      <Harness initial={section({ items: [item("first"), item("second")] })} onDragEnd={onDragEnd} />,
    );

    // The grip is a real button so it is tab-reachable, and dnd-kit's keyboard
    // sensor drives the same drag a pointer would.
    screen.getByRole("button", { name: "Drag to reorder" }).first().element().focus();
    await userEvent.keyboard(" ");
    await userEvent.keyboard("{ArrowDown}");
    await userEvent.keyboard(" ");

    await expect.poll(() => onDragEnd.mock.calls.length).toBe(1);
    const event = onDragEnd.mock.calls[0][0] as DragEndEvent;
    // Rows are registered under their stable keys, so the page's reorder can
    // find them regardless of how the list has been rearranged.
    expect(event.active.id).toBe("first");
    expect(event.over?.id).toBe("second");
  });

  it("hides the editing affordances when the list cannot be written", async () => {
    const screen = await render(
      <Harness initial={section({ items: [item("first")] })} disabled />,
    );

    // Read-only mode is a real deployment: the controls leave the page (and the
    // accessibility tree with it) rather than sitting there and failing.
    expect(screen.getByRole("button", { name: "Remove item" }).elements()).toHaveLength(0);
    expect(screen.getByRole("button", { name: "Drag to reorder" }).elements()).toHaveLength(0);
    // The section menu stays, disabled: its list-style entries are the one part
    // that is still worth seeing when nothing can be changed.
    await expect.element(screen.getByRole("button", { name: "Section actions" })).toBeDisabled();
  });
});
