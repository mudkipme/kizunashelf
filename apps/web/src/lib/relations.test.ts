import { describe, expect, it } from "vitest";

import type { Relation } from "@/types/api";

import { groupRelations, relationKey } from "./relations";

function relation(extra: Partial<Relation>): Relation {
  return { sourceId: "anime:a", targetTitle: "Beta", field: "related", direction: "out", ...extra };
}

describe("groupRelations", () => {
  it("groups by field, preserving first-seen field order and item order", () => {
    const relations = [
      relation({ field: "related", targetTitle: "Beta" }),
      relation({ field: "body", targetTitle: "Gamma" }),
      relation({ field: "related", targetTitle: "Delta" }),
    ];
    expect(groupRelations(relations)).toEqual([
      {
        field: "related",
        items: [
          relation({ field: "related", targetTitle: "Beta" }),
          relation({ field: "related", targetTitle: "Delta" }),
        ],
      },
      { field: "body", items: [relation({ field: "body", targetTitle: "Gamma" })] },
    ]);
  });

  it("returns an empty list for no relations", () => {
    expect(groupRelations([])).toEqual([]);
  });
});

describe("relationKey", () => {
  it("builds a stable key from the relation's identity, with a blank for an unresolved target", () => {
    expect(
      relationKey(
        relation({ sourceId: "anime:a", field: "related", targetTitle: "Beta", direction: "out", targetId: "anime:b" }),
      ),
    ).toBe("anime:a-related-Beta-out-anime:b");
    // An unresolved target (no targetId) yields a trailing empty segment.
    expect(relationKey(relation({ sourceId: "anime:a", field: "related", targetTitle: "Ghost", direction: "out" }))).toBe(
      "anime:a-related-Ghost-out-",
    );
  });
});
