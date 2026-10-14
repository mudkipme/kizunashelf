import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { userEvent } from "vitest/browser";

import {
  duplicateEntityType,
  getLanguages,
  getPathSuggestions,
  getTypePresets,
  resolveTypePresets,
} from "@/api/settings";
import { PresetGallery, PresetPickerDialog } from "@/components/settings/preset-picker";
import { TypesSection } from "@/components/settings/settings-dialogs";
import { useLanguageStore } from "@/lib/language";
import { render } from "@/test/render";
import type { EntityTypeConfig, TypePresetSummary } from "@/types/api";

vi.mock(import("@/api/settings"), async (importOriginal) => ({
  ...(await importOriginal()),
  duplicateEntityType: vi.fn(),
  getPathSuggestions: vi.fn(),
  getLanguages: vi.fn(),
  getTypePresets: vi.fn(),
  resolveTypePresets: vi.fn(),
}));

const source: EntityTypeConfig = {
  id: "drama",
  label: "TV & Drama",
  path: "Drama",
  icon: "🎭",
  fields: [
    { field: "tvdb", fieldType: "externalRef", externalRef: "thetvdb", externalTypes: ["series"] },
    {
      field: "state",
      fieldType: "enum",
      enumRole: "status",
      enumOptions: ["Watching", "Done"],
      statusValues: { ongoing: ["Watching"], completed: ["Done"] },
    },
  ],
  bodySections: [{ heading: "Episodes", kind: "episodes", tracking: "checklist" }],
  log: { lineFormat: "- {title} {note} #TV" },
};
const preview = { ...source, id: "drama-2", label: "TV & Drama 2", path: "Drama 2" };
const copy = { ...source, id: "children_tv", label: "Children's TV", path: "Children/TV" };
const preset: TypePresetSummary = {
  id: "drama",
  label: "TV & Drama",
  icon: "🎭",
  category: "watch",
  description: "TV series",
  providers: [],
  relationTargets: [],
};
const catalog = { presets: [preset], categories: [{ id: "watch" as const, label: "Watch" }] };

function Harness({ onChange }: { onChange: (types: EntityTypeConfig[]) => void }) {
  const [types, setTypes] = useState([source]);
  return (
    <TypesSection
      types={types}
      languages={[]}
      taxonomyBase="/vault/Taxonomy"
      taxonomyRoot="Taxonomy"
      onChange={(next) => {
        setTypes(next);
        onChange(next);
      }}
    />
  );
}

beforeEach(() => {
  useLanguageStore.setState({ language: "en" });
  vi.mocked(duplicateEntityType).mockImplementation((request) =>
    Promise.resolve({ entityType: request.newId ? copy : preview }),
  );
  vi.mocked(getPathSuggestions).mockResolvedValue({ suggestions: [] });
  vi.mocked(getLanguages).mockResolvedValue({
    languages: [{ code: "en" }],
    userLanguages: [{ code: "en", label: "English", titleLanguage: "en" }],
  });
  vi.mocked(getTypePresets).mockResolvedValue(catalog);
  vi.mocked(resolveTypePresets).mockResolvedValue({
    types: [preview],
    backfills: [],
    collisions: [],
  });
});
afterEach(() => vi.resetAllMocks());

describe("duplicate entity type", () => {
  it("copies the applied schema draft and appends only the validated core result", async () => {
    const changed = vi.fn();
    const screen = await render(<Harness onChange={changed} />);
    await screen.getByRole("button", { name: /^TV & Drama/ }).click();
    await screen.getByRole("textbox", { name: "Label", exact: true }).fill("Family TV");
    await screen.getByRole("button", { name: "Apply", exact: true }).click();
    const edited = { ...source, label: "Family TV" };
    expect(changed).toHaveBeenLastCalledWith([edited]);
    changed.mockClear();

    await screen.getByRole("button", { name: "Duplicate Family TV", exact: true }).click();
    await expect
      .element(screen.getByRole("textbox", { name: "ID", exact: true }))
      .toHaveValue("drama-2");
    expect(vi.mocked(duplicateEntityType).mock.lastCall?.[0]).toEqual({
      currentTypes: [edited],
      sourceTypeId: "drama",
    });
    expect(changed).not.toHaveBeenCalled();
    await screen.getByRole("textbox", { name: "Label", exact: true }).fill(copy.label);
    await screen.getByRole("textbox", { name: "ID", exact: true }).fill(copy.id);
    await screen.getByRole("combobox", { name: "Path", exact: true }).fill(copy.path);
    await screen.getByRole("button", { name: "Add", exact: true }).click();
    await expect.poll(() => changed.mock.calls).toEqual([[[edited, copy]]]);
    expect(vi.mocked(duplicateEntityType).mock.lastCall?.[0]).toEqual({
      currentTypes: [edited],
      sourceTypeId: "drama",
      newId: copy.id,
      newLabel: copy.label,
      newPath: copy.path,
    });
    expect(source.label).toBe("TV & Drama");
    await expect.element(screen.getByRole("dialog")).not.toBeInTheDocument();
  });

  it("discards cancelled identity edits when reopened", async () => {
    const changed = vi.fn();
    const screen = await render(<Harness onChange={changed} />);
    await screen.getByRole("button", { name: "Duplicate TV & Drama", exact: true }).click();
    await screen.getByRole("textbox", { name: "Label", exact: true }).fill("Discard me");
    await screen.getByRole("button", { name: "Cancel", exact: true }).click();
    expect(changed).not.toHaveBeenCalled();
    await screen.getByRole("button", { name: "Duplicate TV & Drama", exact: true }).click();
    await expect
      .element(screen.getByRole("textbox", { name: "Label", exact: true }))
      .toHaveValue(preview.label);
  });

  it("ignores a preview that completes after cancellation", async () => {
    let complete!: (result: { entityType: EntityTypeConfig }) => void;
    vi.mocked(duplicateEntityType).mockReturnValue(
      new Promise((resolve) => {
        complete = resolve;
      }),
    );
    const changed = vi.fn();
    const screen = await render(<Harness onChange={changed} />);
    await screen.getByRole("button", { name: "Duplicate TV & Drama", exact: true }).click();
    await expect.element(screen.getByRole("button", { name: "Add", exact: true })).toBeDisabled();
    await screen.getByRole("button", { name: "Cancel", exact: true }).click();
    complete({ entityType: preview });
    await expect.element(screen.getByRole("dialog")).not.toBeInTheDocument();
    expect(changed).not.toHaveBeenCalled();
  });

  it("keeps entered values after a core validation error so the folder can be corrected", async () => {
    vi.mocked(duplicateEntityType).mockImplementation((request) =>
      request.newId
        ? Promise.reject(new Error("Choose a separate folder"))
        : Promise.resolve({ entityType: preview }),
    );
    const changed = vi.fn();
    const screen = await render(<Harness onChange={changed} />);
    await screen.getByRole("button", { name: "Duplicate TV & Drama", exact: true }).click();
    await screen.getByRole("combobox", { name: "Path", exact: true }).fill("Drama/Children");
    await screen.getByRole("button", { name: "Add", exact: true }).click();
    await expect.element(screen.getByRole("alert")).toHaveTextContent("Choose a separate folder");
    await expect
      .element(screen.getByRole("combobox", { name: "Path", exact: true }))
      .toHaveValue("Drama/Children");
    expect(changed).not.toHaveBeenCalled();
    vi.mocked(duplicateEntityType).mockResolvedValue({ entityType: copy });
    await screen.getByRole("combobox", { name: "Path", exact: true }).fill(copy.path);
    await screen.getByRole("button", { name: "Add", exact: true }).click();
    await expect.poll(() => changed.mock.calls).toEqual([[[source, copy]]]);
  });

  it("blocks dismissal while the validated copy is being added", async () => {
    let complete!: (result: { entityType: EntityTypeConfig }) => void;
    vi.mocked(duplicateEntityType).mockImplementation((request) =>
      request.newId
        ? new Promise((resolve) => {
            complete = resolve;
          })
        : Promise.resolve({ entityType: preview }),
    );
    const changed = vi.fn();
    const screen = await render(<Harness onChange={changed} />);
    await screen.getByRole("button", { name: "Duplicate TV & Drama", exact: true }).click();
    await screen.getByRole("button", { name: "Add", exact: true }).click();
    await expect
      .element(screen.getByRole("button", { name: "Cancel", exact: true }))
      .toBeDisabled();
    await userEvent.keyboard("{Escape}");
    await expect.element(screen.getByRole("dialog")).toBeVisible();
    complete({ entityType: copy });
    await expect.poll(() => changed.mock.calls).toEqual([[[source, copy]]]);
  });
});

it("rejects a copy completed after the schema draft changes", async () => {
  let complete!: (result: { entityType: EntityTypeConfig }) => void;
  vi.mocked(duplicateEntityType).mockImplementation((request) =>
    request.newId
      ? new Promise((resolve) => {
          complete = resolve;
        })
      : Promise.resolve({ entityType: preview }),
  );
  const changed = vi.fn();
  const view = (types: EntityTypeConfig[]) => (
    <TypesSection
      types={types}
      languages={[]}
      taxonomyBase="/vault/Taxonomy"
      taxonomyRoot="Taxonomy"
      onChange={changed}
    />
  );
  const screen = await render(view([source]));
  await screen.getByRole("button", { name: "Duplicate TV & Drama", exact: true }).click();
  await screen.getByRole("button", { name: "Add", exact: true }).click();
  await screen.rerender(view([source, copy]));
  complete({ entityType: copy });
  await expect.element(screen.getByRole("alert")).toHaveTextContent("The schema draft changed");
  expect(changed).not.toHaveBeenCalled();
});

describe("repeated built-in types", () => {
  it("lets settings select an already-added preset and resolves it against the draft", async () => {
    const apply = vi.fn();
    const screen = await render(
      <PresetPickerDialog currentTypes={[source]} onClose={vi.fn()} onApply={apply} />,
    );
    const card = screen.getByRole("button", { name: /TV & Drama.*Add another/ });
    await expect.element(card).toBeEnabled();
    await card.click();
    await expect.element(card).toHaveAttribute("aria-pressed", "true");
    await screen.getByRole("button", { name: "Continue (1)", exact: true }).click();
    await expect.poll(() => apply.mock.calls).toEqual([[[source, preview]]]);
    expect(resolveTypePresets).toHaveBeenCalledWith({
      currentTypes: [source],
      presetIds: ["drama"],
      language: "en",
    });
  });

  it("retains the existing disabled gallery behavior unless repeat selection is enabled", async () => {
    const toggle = vi.fn();
    const screen = await render(
      <PresetGallery
        catalog={catalog}
        selected={new Set()}
        existingIds={new Set(["drama"])}
        onToggle={toggle}
      />,
    );
    await expect.element(screen.getByRole("button", { name: /TV & Drama.*Added/ })).toBeDisabled();
    expect(toggle).not.toHaveBeenCalled();
  });
});
