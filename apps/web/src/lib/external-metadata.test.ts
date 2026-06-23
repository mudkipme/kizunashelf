import { describe, expect, it } from "vitest";

import type { FieldType } from "@/lib/type-config";
import type { ExternalCandidate, FieldConfig, TypeConfig } from "@/types/api";

import {
  applyExternalBodySections,
  candidateMetadataEntries,
  candidateMetadataPatch,
  candidateMetadataPreviewEntries,
  externalBodySectionState,
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

// Only `field`/`fieldType` are required; the rest are filled per-case.
function field(extra: Partial<FieldConfig> & { fieldType: FieldType }): FieldConfig {
  return { field: "x", ...extra } as FieldConfig;
}

function typeConfig(fields: FieldConfig[]): TypeConfig {
  return { id: "anime", label: "Anime", path: "Anime", fields };
}

function candidate(extra: Partial<ExternalCandidate>): ExternalCandidate {
  return {
    provider: "bangumi",
    sourceId: "123",
    url: "https://bgm.tv/subject/123",
    title: "Star Voyager",
    metadata: {},
    ...extra,
  };
}

describe("candidateMetadataPreviewEntries", () => {
  it("maps an externalRef field to the candidate URL when the provider matches", () => {
    const tc = typeConfig([
      field({ field: "bangumi_id", fieldType: "externalRef", externalRef: "bangumi" }),
    ]);
    const entries = candidateMetadataPreviewEntries(
      candidate({ provider: "bangumi", url: "https://bgm.tv/subject/123" }),
      tc,
    );
    expect(entries).toHaveLength(1);
    expect(entries[0]).toMatchObject({
      field: "bangumi_id",
      value: "https://bgm.tv/subject/123",
      source: "bangumi",
      hasValue: true,
    });
  });

  it("does not map an externalRef field when the provider differs", () => {
    const tc = typeConfig([
      field({ field: "bangumi_id", fieldType: "externalRef", externalRef: "bangumi" }),
    ]);
    expect(candidateMetadataPreviewEntries(candidate({ provider: "igdb" }), tc)).toEqual([]);
  });

  it("maps a scalar field from its external field, keyed off the schema not the field name", () => {
    const tc = typeConfig([
      field({ field: "name_jp", fieldType: "title", externalFields: [{ source: "bangumi", field: "name" }] }),
    ]);
    const entries = candidateMetadataPreviewEntries(
      candidate({ metadata: { name: "スターボイジャー" } }),
      tc,
    );
    expect(entries[0]).toMatchObject({
      field: "name_jp",
      externalField: "name",
      value: "スターボイジャー",
      hasValue: true,
    });
  });

  it("matches the mapping source case-insensitively", () => {
    const tc = typeConfig([
      field({ field: "name", fieldType: "title", externalFields: [{ source: "BANGUMI", field: "name" }] }),
    ]);
    const entries = candidateMetadataPreviewEntries(
      candidate({ provider: "bangumi", metadata: { name: "SV" } }),
      tc,
    );
    expect(entries[0]?.value).toBe("SV");
  });

  it("flags a missing value as hasValue=false rather than omitting the entry", () => {
    const tc = typeConfig([
      field({ field: "name", fieldType: "title", externalFields: [{ source: "bangumi", field: "name" }] }),
    ]);
    const entries = candidateMetadataPreviewEntries(candidate({ metadata: {} }), tc);
    expect(entries).toHaveLength(1);
    expect(entries[0].hasValue).toBe(false);
  });
});

describe("normalizeValueForField (via candidateMetadataEntries)", () => {
  it("keeps an array for a list field and wraps a scalar into a single-item list", () => {
    const tc = typeConfig([
      field({ field: "genres", fieldType: "enumList", externalFields: [{ source: "bangumi", field: "tags" }] }),
    ]);
    expect(candidateMetadataEntries(candidate({ metadata: { tags: ["SF", "Space"] } }), tc)[0].value).toEqual([
      "SF",
      "Space",
    ]);
    expect(candidateMetadataEntries(candidate({ metadata: { tags: "SF" } }), tc)[0].value).toEqual(["SF"]);
  });

  it("flattens a list onto a scalar field as comma-joined text, dropping empties", () => {
    const tc = typeConfig([
      field({ field: "studio", fieldType: "text", externalFields: [{ source: "bangumi", field: "studios" }] }),
    ]);
    expect(candidateMetadataEntries(candidate({ metadata: { studios: ["A", "", "B"] } }), tc)[0].value).toBe(
      "A, B",
    );
  });

  it("drops entries with no value from candidateMetadataEntries", () => {
    const tc = typeConfig([
      field({ field: "name", fieldType: "title", externalFields: [{ source: "bangumi", field: "name" }] }),
    ]);
    expect(candidateMetadataEntries(candidate({ metadata: {} }), tc)).toHaveLength(0);
  });
});

describe("candidateMetadataPatch", () => {
  it("includes only the selected fields", () => {
    const tc = typeConfig([
      field({ field: "name", fieldType: "title", externalFields: [{ source: "bangumi", field: "name" }] }),
      field({ field: "genres", fieldType: "enumList", externalFields: [{ source: "bangumi", field: "tags" }] }),
    ]);
    const cand = candidate({ metadata: { name: "SV", tags: ["A"] } });
    expect(candidateMetadataPatch(cand, tc, new Set(["name"]))).toEqual({ name: "SV" });
    expect(candidateMetadataPatch(cand, tc, new Set(["name", "genres"]))).toEqual({
      name: "SV",
      genres: ["A"],
    });
  });
});
