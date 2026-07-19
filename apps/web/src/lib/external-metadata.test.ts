import { describe, expect, it } from "vitest";

import type { ExternalMatch, FieldConfig, MappedFieldValue, TypeConfig } from "@/types/api";

import { matchFieldPatch, matchFieldPreviewEntries } from "./external-metadata";

// The core resolves each field's value, the selection policy, and the body
// splice (`reviewExternalCandidate`/`applyExternalCandidate`); the client only
// re-labels entries for display and derives the cover-download heuristic.
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
      title: "Steins;Gate 0",
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
      match([fieldValue({ field: "name_jp", value: "シュタインズ・ゲート ゼロ", externalField: "name" })]),
      tc,
    );
    expect(entries).toHaveLength(1);
    expect(entries[0]).toMatchObject({
      field: "name_jp",
      label: "Japanese title",
      value: "シュタインズ・ゲート ゼロ",
      externalField: "name",
      hasValue: true,
    });
  });

  it("falls back to the field name when the schema has no matching field", () => {
    const entries = matchFieldPreviewEntries(match([fieldValue({ field: "mystery", value: "x" })]), typeConfig([]));
    expect(entries[0].label).toBe("mystery");
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
