import { describe, expect, it } from "vitest";

import type { ExternalMatch, FieldConfig, MappedFieldValue, TypeConfig } from "@/types/api";

import {
  applyExternalBodySections,
  bodySectionSelectionState,
  externalBodySectionState,
  fieldSelectionState,
  matchBodyPatch,
  matchDefaultBodySections,
  matchDefaultFields,
  matchFieldPatch,
  matchFieldPreviewEntries,
  type ExternalBodyPatch,
} from "./external-metadata";

// Only `heading`/`markdown` drive the body rewrite; key/source/field exist for
// dedup/provenance, so fill them with placeholders here.
function patch(heading: string, markdown: string): ExternalBodyPatch {
  return { key: `${heading}:${markdown}`, source: "test", field: "summary", heading, markdown };
}

describe("applyExternalBodySections", () => {
  it("appends a new section when the heading is absent", () => {
    const result = applyExternalBodySections("Existing notes.", [patch("Summary", "Hello world")]);
    expect(result).toBe("Existing notes.\n\n## Summary\n\nHello world\n");
  });

  it("appends to an empty body without leading blank lines", () => {
    const result = applyExternalBodySections("", [patch("Summary", "Hello")]);
    expect(result).toBe("## Summary\n\nHello\n");
  });

  it("replaces an existing section's content while preserving following sections", () => {
    const body = "## Summary\n\nOld text\n\n## Other\n\nKeep\n";
    const result = applyExternalBodySections(body, [patch("Summary", "New")]);
    expect(result).toBe("## Summary\n\nNew\n\n## Other\n\nKeep\n");
  });

  it("matches the heading case-insensitively", () => {
    const body = "## Summary\n\nOld\n";
    const result = applyExternalBodySections(body, [patch("summary", "New")]);
    expect(result).toBe("## Summary\n\nNew\n");
  });

  it("applies multiple patches in order", () => {
    const result = applyExternalBodySections("", [patch("Summary", "S"), patch("Story", "T")]);
    expect(result).toBe("## Summary\n\nS\n\n## Story\n\nT\n");
  });

  it("ignores a heading that only appears inside a fenced code block", () => {
    // The `## Summary` line lives inside a ``` fence, so it isn't a real heading:
    // the section must be appended, not treated as already present.
    const body = "```\n## Summary\n```\n\nbody\n";
    const result = applyExternalBodySections(body, [patch("Summary", "New")]);
    expect(result).toBe("```\n## Summary\n```\n\nbody\n\n## Summary\n\nNew\n");
  });
});

describe("externalBodySectionState", () => {
  it("reports replace when the heading exists and append when it doesn't", () => {
    const body = "## Notes\n\nhi\n";
    expect(externalBodySectionState(body, "Notes")).toBe("replace");
    expect(externalBodySectionState(body, "notes")).toBe("replace");
    expect(externalBodySectionState(body, "Summary")).toBe("append");
  });

  it("treats a heading inside a code fence as absent", () => {
    const body = "```\n## Summary\n```\n\n## Notes\n\nhi\n";
    expect(externalBodySectionState(body, "Summary")).toBe("append");
    expect(externalBodySectionState(body, "Notes")).toBe("replace");
  });
});

// The core resolves each field's value; the client only re-labels and selects.
function fieldValue(extra: Partial<MappedFieldValue> & { field: string }): MappedFieldValue {
  return { value: null, source: "bangumi", hasValue: true, ...extra };
}

function match(
  fields: MappedFieldValue[],
  bodySections: ExternalMatch["bodySections"] = [],
): ExternalMatch {
  return {
    entityType: "anime",
    candidate: {
      provider: "bangumi",
      sourceId: "123",
      url: "https://bgm.tv/subject/123",
      title: "Star Voyager",
      titles: {},
      metadata: {},
    },
    fields,
    bodySections,
  };
}

function field(extra: Partial<FieldConfig> & { field: string }): FieldConfig {
  return { fieldType: "text", ...extra } as FieldConfig;
}

function typeConfig(fields: FieldConfig[]): TypeConfig {
  return { id: "anime", label: "Anime", path: "Anime", fields };
}

describe("matchFieldPreviewEntries", () => {
  it("decorates each core-mapped field with its schema label", () => {
    const tc = typeConfig([field({ field: "name_jp", fieldType: "title", displayName: "Japanese title" })]);
    const entries = matchFieldPreviewEntries(
      match([fieldValue({ field: "name_jp", value: "スターボイジャー", externalField: "name" })]),
      tc,
    );
    expect(entries).toHaveLength(1);
    expect(entries[0]).toMatchObject({
      field: "name_jp",
      label: "Japanese title",
      value: "スターボイジャー",
      externalField: "name",
      hasValue: true,
    });
  });

  it("falls back to the field name when the schema has no matching field", () => {
    const entries = matchFieldPreviewEntries(match([fieldValue({ field: "mystery", value: "x" })]), typeConfig([]));
    expect(entries[0].label).toBe("mystery");
  });
});

describe("fieldSelectionState", () => {
  it("locks a valueless field off", () => {
    expect(fieldSelectionState(fieldValue({ field: "x", value: [], hasValue: false }), undefined)).toEqual({
      checked: false,
      locked: true,
    });
  });

  it("locks the external ref field on (its externalField is absent)", () => {
    expect(
      fieldSelectionState(fieldValue({ field: "ref", value: "https://bgm.tv/subject/123" }), "https://old"),
    ).toEqual({ checked: true, locked: true });
  });

  it("locks the external ref off when it already matches (re-matching the same candidate)", () => {
    expect(
      fieldSelectionState(
        fieldValue({ field: "ref", value: "https://bgm.tv/subject/123" }),
        "https://bgm.tv/subject/123",
      ),
    ).toEqual({ checked: false, locked: true });
  });

  it("locks a field whose value already matches the current one off", () => {
    expect(fieldSelectionState(fieldValue({ field: "name", value: "SV", externalField: "name" }), "SV")).toEqual({
      checked: false,
      locked: true,
    });
  });

  it("checks an empty current field by default and leaves it editable", () => {
    expect(fieldSelectionState(fieldValue({ field: "name", value: "SV", externalField: "name" }), "")).toEqual({
      checked: true,
      locked: false,
    });
    expect(fieldSelectionState(fieldValue({ field: "tags", value: ["a"], externalField: "tags" }), [])).toEqual({
      checked: true,
      locked: false,
    });
  });

  it("leaves a differing populated field unchecked but editable", () => {
    expect(
      fieldSelectionState(fieldValue({ field: "name", value: "SV", externalField: "name" }), "Other"),
    ).toEqual({ checked: false, locked: false });
  });
});

describe("matchDefaultFields", () => {
  it("defaults empty and ref fields on, same and existing off", () => {
    const candidate = match([
      fieldValue({ field: "ref", value: "https://bgm.tv/subject/123" }), // ref -> on
      fieldValue({ field: "name", value: "SV", externalField: "name" }), // empty current -> on
      fieldValue({ field: "year", value: "2026", externalField: "year" }), // same -> off
      fieldValue({ field: "note", value: "New", externalField: "note" }), // existing differs -> off
      fieldValue({ field: "empty", value: [], hasValue: false }), // no value -> off
    ]);
    const current = { name: "", year: "2026", note: "Old" };
    expect(matchDefaultFields(candidate, current).sort()).toEqual(["name", "ref"]);
  });
});

function bodySection(extra: Partial<NonNullable<ExternalMatch["bodySections"]>[number]> = {}) {
  return {
    key: "Summary:bangumi:summary",
    heading: "Summary",
    source: "bangumi",
    externalField: "summary",
    markdown: "New text",
    hasValue: true,
    ...extra,
  };
}

describe("bodySectionSelectionState", () => {
  it("checks an absent section, unchecks a differing one, locks an identical one", () => {
    expect(bodySectionSelectionState(bodySection(), undefined)).toEqual({ checked: true, locked: false });
    expect(bodySectionSelectionState(bodySection(), "## Other\n\nx\n")).toEqual({ checked: true, locked: false });
    expect(bodySectionSelectionState(bodySection(), "## Summary\n\nOld\n")).toEqual({ checked: false, locked: false });
    expect(bodySectionSelectionState(bodySection(), "## Summary\n\nNew text\n")).toEqual({
      checked: false,
      locked: true,
    });
    expect(bodySectionSelectionState(bodySection({ hasValue: false }), undefined)).toEqual({
      checked: false,
      locked: true,
    });
  });
});

describe("matchDefaultBodySections", () => {
  it("defaults only new or absent sections on", () => {
    const candidate = match(
      [],
      [
        bodySection(), // identical existing -> off
        bodySection({ key: "Notes:bangumi:notes", heading: "Notes", externalField: "notes", markdown: "N" }),
      ],
    );
    expect(matchDefaultBodySections(candidate, "## Summary\n\nNew text\n")).toEqual(["Notes:bangumi:notes"]);
  });
});

describe("matchFieldPatch", () => {
  it("includes only selected fields that carry a value", () => {
    const candidate = match([
      fieldValue({ field: "name", value: "SV" }),
      fieldValue({ field: "season", value: ["2026年春季"] }),
      fieldValue({ field: "empty", value: [], hasValue: false }),
    ]);
    expect(matchFieldPatch(candidate, new Set(["name"]))).toEqual({ name: "SV" });
    expect(matchFieldPatch(candidate, new Set(["name", "season", "empty"]))).toEqual({
      name: "SV",
      season: ["2026年春季"],
    });
  });
});

describe("matchBodyPatch", () => {
  it("maps selected body sections to heading/markdown patches", () => {
    const candidate = match(
      [],
      [
        {
          key: "Summary:bangumi:summary",
          heading: "Summary",
          source: "bangumi",
          externalField: "summary",
          markdown: "A long voyage.",
          hasValue: true,
        },
      ],
    );
    expect(matchBodyPatch(candidate, new Set(["Summary:bangumi:summary"]))).toEqual([
      {
        key: "Summary:bangumi:summary",
        source: "bangumi",
        field: "summary",
        heading: "Summary",
        markdown: "A long voyage.",
      },
    ]);
    expect(matchBodyPatch(candidate, new Set())).toEqual([]);
  });
});
