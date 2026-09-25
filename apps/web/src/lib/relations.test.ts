import { describe, expect, it } from "vitest";

import type { Relation } from "@/types/api";

import { groupRelations } from "./relations";

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
});
