import { describe, expect, it } from "vitest";

import type { EntitySummary, TypeConfig } from "@/types/api";

import { editableFieldSpecs } from "./metadata-field-specs";

describe("editableFieldSpecs relation suggestions", () => {
  const typeConfig = {
    id: "anime",
    label: "Anime",
    path: "Anime",
    fields: [{ field: "related", fieldType: "relation" }],
  } as unknown as TypeConfig;

  const suggestion = {
    basename: "Steins;Gate 0 (Anime)",
    title: "Steins;Gate 0",
    titles: { ja: "シュタインズ・ゲート ゼロ" },
    type: "anime",
    typeLabel: "Anime",
  } as unknown as EntitySummary;

  function relationOptions(language: string) {
    const specs = editableFieldSpecs(typeConfig, {}, [suggestion], undefined, language);
    return specs.find((spec) => spec.key === "related")?.relationOptions ?? [];
  }

  it("labels suggestions with the viewer's language title (value stays the basename)", () => {
    expect(relationOptions("ja")).toEqual([
      { value: "Steins;Gate 0 (Anime)", label: "シュタインズ・ゲート ゼロ" },
    ]);
  });

  it("falls back to the canonical title when the viewer's language is missing", () => {
    expect(relationOptions("en")).toEqual([
      { value: "Steins;Gate 0 (Anime)", label: "Steins;Gate 0" },
    ]);
  });
});

describe("editableFieldSpecs built-in tags (opt-in)", () => {
  it("claims the configured tags key up front when the feature is enabled", () => {
    const specs = editableFieldSpecs(undefined, {}, [], undefined, "en", ["cozy"], "labels");
    expect(specs[0]).toMatchObject({
      key: "labels",
      kind: "list",
      options: ["cozy"],
      configured: true,
    });
  });

  it("treats a frontmatter key named tags as an ordinary unknown field when disabled", () => {
    const specs = editableFieldSpecs(
      undefined,
      { tags: ["cozy"] },
      [],
      undefined,
      "en",
      [],
      undefined,
    );
    expect(specs).toHaveLength(1);
    expect(specs[0]).toMatchObject({ key: "tags", kind: "list", configured: false });
  });
});
