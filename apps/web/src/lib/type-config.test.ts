import { describe, expect, it } from "vitest";

import {
  configuredFieldLabel,
  entityFieldLabel,
  type FieldConfig,
  type FieldType,
  fieldTypeLabel,
  isDateFieldType,
  isListFieldType,
  supportsDateRole,
  supportsEnumOptions,
} from "./type-config";

describe("field-type role predicates", () => {
  it("isListFieldType is true only for list-shaped fields", () => {
    const list: FieldType[] = ["imageList", "enumList", "season", "relation", "textList"];
    const scalar: FieldType[] = ["title", "image", "enum", "date", "text", "id", "rating", "bool"];
    for (const type of list) expect(isListFieldType(type)).toBe(true);
    for (const type of scalar) expect(isListFieldType(type)).toBe(false);
  });

  it("isDateFieldType / supportsDateRole cover date and season", () => {
    expect(isDateFieldType("date")).toBe(true);
    expect(isDateFieldType("season")).toBe(true);
    expect(isDateFieldType("text")).toBe(false);
    expect(supportsDateRole("season")).toBe(true);
    expect(supportsDateRole("text")).toBe(false);
  });

  it("supportsEnumOptions covers enum and enumList", () => {
    expect(supportsEnumOptions("enum")).toBe(true);
    expect(supportsEnumOptions("enumList")).toBe(true);
    expect(supportsEnumOptions("text")).toBe(false);
  });
});

describe("fieldTypeLabel", () => {
  it("maps known types and falls back to Text", () => {
    expect(fieldTypeLabel("id")).toBe("ID");
    expect(fieldTypeLabel("imageList")).toBe("Image list");
    expect(fieldTypeLabel("externalRef")).toBe("External ref");
    expect(fieldTypeLabel("text")).toBe("Text");
  });
});

describe("configuredFieldLabel", () => {
  const field = (extra: Partial<FieldConfig>): FieldConfig =>
    ({ field: "x", fieldType: "text", ...extra }) as FieldConfig;

  it("prefers an explicit (trimmed) displayName", () => {
    expect(configuredFieldLabel(field({ displayName: "  Cover  " }))).toBe("Cover");
  });

  it("describes title roles and languages by the schema, not the field name", () => {
    expect(
      configuredFieldLabel(field({ field: "name", fieldType: "title", titleRole: "original" })),
    ).toBe("Original title: name");
    expect(
      configuredFieldLabel(field({ field: "name_jp", fieldType: "title", titleLanguage: "ja" })),
    ).toBe("ja title: name_jp");
  });

  it("describes date roles", () => {
    expect(
      configuredFieldLabel(field({ field: "watched", fieldType: "date", dateRole: "completed" })),
    ).toBe("Completed date: watched");
    expect(configuredFieldLabel(field({ field: "plan", fieldType: "date" }))).toBe(
      "Planning date: plan",
    );
  });

  it("falls back to the type label for plain fields", () => {
    expect(configuredFieldLabel(field({ field: "rating", fieldType: "rating" }))).toBe(
      "Rating: rating",
    );
  });
});

describe("entityFieldLabel", () => {
  it("reads the untyped body pseudo-field as Notes (e.g. inbound 'Linked from')", () => {
    expect(entityFieldLabel(undefined, "anime", "body")).toBe("Notes");
    const labels = new Map([["anime", new Map([["studio", "Studio"]])]]);
    expect(entityFieldLabel(labels, "anime", "body")).toBe("Notes");
  });

  it("prefers a real configured field named body over the Notes fallback", () => {
    const labels = new Map([["anime", new Map([["body", "Synopsis"]])]]);
    expect(entityFieldLabel(labels, "anime", "body")).toBe("Synopsis");
  });

  it("uses the configured label, else the raw field name", () => {
    const labels = new Map([["anime", new Map([["studio", "Studio"]])]]);
    expect(entityFieldLabel(labels, "anime", "studio")).toBe("Studio");
    expect(entityFieldLabel(labels, "anime", "unknown")).toBe("unknown");
  });
});
