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
    basename: "Star Voyager",
    title: "Star Voyager",
    titles: { ja: "スターボイジャー" },
    type: "anime",
    typeLabel: "Anime",
  } as unknown as EntitySummary;

  function relationOptions(language: string) {
    const specs = editableFieldSpecs(typeConfig, {}, [suggestion], undefined, language);
    return specs.find((spec) => spec.key === "related")?.relationOptions ?? [];
  }

  it("labels suggestions with the viewer's language title (value stays the basename)", () => {
    expect(relationOptions("ja")).toEqual([{ value: "Star Voyager", label: "スターボイジャー" }]);
  });

  it("falls back to the canonical title when the viewer's language is missing", () => {
    expect(relationOptions("en")).toEqual([{ value: "Star Voyager", label: "Star Voyager" }]);
  });
});
