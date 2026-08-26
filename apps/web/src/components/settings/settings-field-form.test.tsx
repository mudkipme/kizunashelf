import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { userEvent } from "vitest/browser";

import { FieldForm } from "@/components/settings/settings-field-form";
import { TitleLanguagesContext } from "@/components/settings/settings-shared";
import { render } from "@/test/render";
import type { ExternalProviderCatalog, FieldConfig, Language } from "@/types/api";

// A `Language` is just its code; the picker labels it from the code itself, so
// the endonym shown is the app's, not the server's.
const languages: Language[] = [{ code: "en" }, { code: "ja" }];

const catalog: ExternalProviderCatalog = {
  providers: [
    {
      id: "tmdb",
      label: "TMDB",
      fields: [{ field: "overview", label: "Overview" }],
      types: [
        { value: "movie", label: "Movie" },
        { value: "tv", label: "TV series" },
      ],
      defaultExternalTypes: ["movie", "tv"],
      credentials: [],
      searchSupported: true,
    },
    {
      id: "bangumi",
      label: "Bangumi",
      fields: [],
      types: [],
      defaultExternalTypes: [],
      credentials: [],
      searchSupported: true,
    },
  ],
};

/**
 * The dialog owns the field being edited, so the form is controlled. Holding it
 * here lets a test make a second edit on top of the first — mapping two enum
 * options in turn, or changing the type and then its new options.
 */
function Controlled({
  initial,
  onChange,
  providerCatalog,
}: {
  initial: FieldConfig;
  onChange?: (field: FieldConfig) => void;
  providerCatalog?: ExternalProviderCatalog;
}) {
  const [field, setField] = useState(initial);
  return (
    <TitleLanguagesContext.Provider value={languages}>
      <FieldForm
        field={field}
        providerCatalog={providerCatalog}
        onChange={(next) => {
          setField(next);
          onChange?.(next);
        }}
      />
    </TitleLanguagesContext.Provider>
  );
}

type Screen = Awaited<ReturnType<typeof render>>;

/**
 * The per-option status selects carry no accessible name of their own — they are
 * identified by the option label sitting beside them in the row.
 */
function statusSelectFor(screen: Screen, option: string) {
  const row = screen.getByText(option, { exact: true }).element().parentElement;
  const select = row?.querySelector("select");
  if (!select) throw new Error(`no status select beside "${option}"`);
  return select;
}

describe("FieldForm", () => {
  it("keys its option editors off the declared type, never the field's name", async () => {
    const screen = await render(
      <div>
        {/* Named like a status, declared as free text: no enum machinery. */}
        <Controlled initial={{ field: "status", fieldType: "text" }} />
        {/* And the reverse — an enum that happens to be called "notes". */}
        <Controlled initial={{ field: "notes", fieldType: "enum", enumOptions: ["Watching"] }} />
      </div>,
    );

    // Exactly one of the two fields offers enum options and a status role, and
    // it is the one whose `fieldType` says `enum`.
    expect(screen.getByText("Enum options").elements()).toHaveLength(1);
    expect(screen.getByRole("combobox", { name: "Used as" }).elements()).toHaveLength(1);
    // Both get provider mappings: every type can be filled from a provider.
    expect(screen.getByText("External field mappings").elements()).toHaveLength(2);
  });

  it("swaps the option editors when the type changes", async () => {
    const screen = await render(
      <Controlled initial={{ field: "status", fieldType: "enum", enumOptions: ["Watching"] }} />,
    );

    await expect.element(screen.getByText("Enum options")).toBeVisible();

    await screen.getByRole("combobox", { name: "Type" }).selectOptions("Date");

    expect(screen.getByText("Enum options").elements()).toHaveLength(0);
    // A date's "Used as" is the date-role vocabulary, not the enum one — same
    // label, entirely different meaning, chosen by the declared type.
    const role = screen.getByRole("combobox", { name: "Used as" });
    expect(role.getByRole("option").elements().map((option) => option.textContent)).toEqual([
      "None",
      "Planning",
      "Started",
      "Completed",
      "Event",
    ]);
  });

  it("gives a season field both a date role and the language its names are written in", async () => {
    const onChange = vi.fn();
    const screen = await render(
      <Controlled initial={{ field: "aired", fieldType: "season" }} onChange={onChange} />,
    );

    await expect.element(screen.getByRole("combobox", { name: "Used as" })).toBeVisible();

    // Season names are stored as the words a vault actually writes ("2026年春"),
    // so the language is part of the schema, not a display preference.
    const language = screen.getByRole("combobox", { name: "Season language" });
    await expect.element(language).toHaveValue("zh");
    await language.selectOptions("Japanese");
    expect(onChange).toHaveBeenLastCalledWith(expect.objectContaining({ seasonLanguage: "ja" }));
  });

  it("seeds a provider's default external types when the provider is picked", async () => {
    const onChange = vi.fn();
    const screen = await render(
      <Controlled
        initial={{ field: "tmdb_id", fieldType: "externalRef" }}
        providerCatalog={catalog}
        onChange={onChange}
      />,
    );

    await screen.getByRole("combobox", { name: "Provider" }).selectOptions("TMDB");

    expect(onChange).toHaveBeenLastCalledWith(
      expect.objectContaining({ externalRef: "tmdb", externalTypes: ["movie", "tv"] }),
    );
    // An external-ref field *is* the provider link; it has no per-field mapping
    // of its own to configure.
    expect(screen.getByText("External field mappings").elements()).toHaveLength(0);
  });

  it("rebuilds the status mapping in schema order, whatever order it was edited in", async () => {
    const onChange = vi.fn();
    const screen = await render(
      <Controlled
        initial={{
          field: "status",
          fieldType: "enum",
          enumRole: "status",
          enumOptions: ["Watching", "Rewatching", "Done"],
        }}
        onChange={onChange}
      />,
    );

    // Mapped back to front, on purpose.
    await userEvent.selectOptions(statusSelectFor(screen, "Rewatching"), "Ongoing");
    await userEvent.selectOptions(statusSelectFor(screen, "Watching"), "Ongoing");
    await userEvent.selectOptions(statusSelectFor(screen, "Done"), "Completed");

    // The mapping is rebuilt in `enumOptions` order, so "Watching" leads
    // `ongoing` — and a log that flips an entity to "ongoing" writes that word,
    // not the "Rewatching" the user happened to map first.
    expect(onChange).toHaveBeenLastCalledWith(
      expect.objectContaining({
        statusValues: {
          planning: [],
          ongoing: ["Watching", "Rewatching"],
          paused: [],
          completed: ["Done"],
          dropped: [],
        },
      }),
    );
  });

  it("drops the mapping when the field stops being the status field", async () => {
    const onChange = vi.fn();
    const screen = await render(
      <Controlled
        initial={{
          field: "status",
          fieldType: "enum",
          enumRole: "status",
          enumOptions: ["Watching"],
          statusValues: { planning: [], ongoing: ["Watching"], paused: [], completed: [], dropped: [] },
        }}
        onChange={onChange}
      />,
    );

    await screen.getByRole("combobox", { name: "Used as" }).selectOptions("Regular enum");

    // A mapping onto a field that is no longer a status field would be dead
    // config that silently comes back if the role is re-enabled.
    expect(onChange).toHaveBeenLastCalledWith(
      expect.objectContaining({ enumRole: undefined, statusValues: undefined }),
    );
    expect(screen.getByText("Map each option to a status").elements()).toHaveLength(0);
  });

  it("says what is missing when a status field has no options to map", async () => {
    const screen = await render(
      <Controlled initial={{ field: "status", fieldType: "enum", enumRole: "status" }} />,
    );

    await expect
      .element(screen.getByText("Add enum options above to map them to statuses."))
      .toBeVisible();
  });

  it("keeps a title language the app does not recognize", async () => {
    const screen = await render(
      <Controlled
        // Hand-written config, or a language this build predates.
        initial={{ field: "name_xx", fieldType: "title", titleLanguage: "xx" }}
      />,
    );

    const language = screen.getByRole("combobox", { name: "Title language" });
    await expect.element(language).toHaveValue("xx");
    // Preserved as its own option rather than snapping to "None" — editing an
    // unrelated part of the field must not silently rewrite this one.
    expect(language.getByRole("option").elements().map((option) => option.textContent)).toEqual([
      "None",
      "English (en)",
      "Japanese (ja)",
      "xx",
    ]);
  });
});
