import { describe, expect, it } from "vitest";

import { visiblePages } from "./pagination";

describe("visiblePages", () => {
  it("lists every page when there are 7 or fewer", () => {
    expect(visiblePages(1, 1)).toEqual([1]);
    expect(visiblePages(3, 7)).toEqual([1, 2, 3, 4, 5, 6, 7]);
  });

  it("collapses the gap after the first page near the start", () => {
    expect(visiblePages(1, 10)).toEqual([1, 2, "ellipsis", 10]);
  });

  it("shows ellipses on both sides in the middle", () => {
    expect(visiblePages(5, 10)).toEqual([1, "ellipsis", 4, 5, 6, "ellipsis", 10]);
  });

  it("collapses the gap before the last page near the end", () => {
    expect(visiblePages(10, 10)).toEqual([1, "ellipsis", 9, 10]);
  });

  it("does not insert an ellipsis for an adjacent gap of one", () => {
    // page 2 of 10: {1,2,3,10} — 1→2→3 are contiguous, only 3→10 gets an ellipsis.
    expect(visiblePages(2, 10)).toEqual([1, 2, 3, "ellipsis", 10]);
  });
});
