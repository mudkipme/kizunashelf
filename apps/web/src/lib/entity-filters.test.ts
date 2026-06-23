import { describe, expect, it } from "vitest";

import type { EntitySummary } from "@/types/api";

import {
  allEntityFilter,
  compareEntitiesByTypeThenTitle,
  entityDateOptions,
  entityMatchesDate,
  entityMatchesQuery,
  entityTypeOptions,
} from "./entity-filters";

function summary(extra: Partial<EntitySummary>): EntitySummary {
  return {
    id: "anime:x",
    type: "anime",
    typeLabel: "Anime",
    title: "Untitled",
    titles: {},
    dates: [],
    path: "Taxonomy/Anime/Untitled.md",
    basename: "Untitled",
    externalRefs: {},
    relationCount: 0,
    ...extra,
  };
}

describe("entityMatchesQuery", () => {
  const entity = summary({
    title: "Star Voyager",
    summary: "a space opera",
    basename: "star-voyager",
    path: "Taxonomy/Anime/star-voyager.md",
    titles: { ja: "スターボイジャー", en: "Star Voyager" },
  });

  it("treats a blank query as a match", () => {
    expect(entityMatchesQuery(entity, "")).toBe(true);
    expect(entityMatchesQuery(entity, "   ")).toBe(true);
  });

  it("matches across title, summary, basename, path, and alternate titles, case-insensitively", () => {
    expect(entityMatchesQuery(entity, "voyager")).toBe(true);
    expect(entityMatchesQuery(entity, "SPACE")).toBe(true);
    expect(entityMatchesQuery(entity, "star-voyager")).toBe(true);
    expect(entityMatchesQuery(entity, "スター")).toBe(true);
    expect(entityMatchesQuery(entity, "nope")).toBe(false);
  });

  it("matches supplied extra values (e.g. enum/relation field values)", () => {
    expect(entityMatchesQuery(summary({ title: "X" }), "sci-fi", ["Sci-Fi"])).toBe(true);
  });
});

describe("entityMatchesDate", () => {
  const dated = summary({ dates: [{ field: "aired", value: "2023-05-01" }] });
  const undated = summary({ dates: [] });

  it("handles all / dated / undated / year:", () => {
    expect(entityMatchesDate(dated, allEntityFilter)).toBe(true);
    expect(entityMatchesDate(dated, "dated")).toBe(true);
    expect(entityMatchesDate(undated, "dated")).toBe(false);
    expect(entityMatchesDate(undated, "undated")).toBe(true);
    expect(entityMatchesDate(dated, "year:2023")).toBe(true);
    expect(entityMatchesDate(dated, "year:2021")).toBe(false);
  });
});

describe("entityTypeOptions", () => {
  it("counts by type, labels from the entity, and sorts by label", () => {
    const entities = [
      summary({ type: "anime", typeLabel: "Anime" }),
      summary({ type: "anime", typeLabel: "Anime" }),
      summary({ type: "game", typeLabel: "Game" }),
    ];
    expect(entityTypeOptions(entities)).toEqual([
      { value: "anime", label: "Anime", count: 2 },
      { value: "game", label: "Game", count: 1 },
    ]);
  });
});

describe("entityDateOptions", () => {
  it("counts distinct years per entity, newest first", () => {
    const entities = [
      summary({
        dates: [
          { field: "a", value: "2023-01-01" },
          { field: "b", value: "2023-12-31" },
        ],
      }),
      summary({ dates: [{ field: "a", value: "2021-06-01" }] }),
    ];
    // The first entity has two 2023 dates but contributes one to the 2023 count.
    expect(entityDateOptions(entities)).toEqual([
      { value: "year:2023", label: "2023", count: 1 },
      { value: "year:2021", label: "2021", count: 1 },
    ]);
  });
});

describe("compareEntitiesByTypeThenTitle", () => {
  it("orders by type label, then title", () => {
    const a = summary({ typeLabel: "Anime", title: "Beta" });
    const b = summary({ typeLabel: "Anime", title: "Alpha" });
    const c = summary({ typeLabel: "Game", title: "Aaa" });
    expect([a, b, c].sort(compareEntitiesByTypeThenTitle).map((entity) => entity.title)).toEqual([
      "Alpha",
      "Beta",
      "Aaa",
    ]);
  });
});
