import { describe, expect, it } from "vitest";

import {
  SIDEBAR_DEFAULT_WIDTH,
  SIDEBAR_MAX_WIDTH,
  SIDEBAR_MIN_WIDTH,
  clampSidebarWidth,
} from "@/lib/sidebar";

describe("clampSidebarWidth", () => {
  it("holds the width inside the usable range", () => {
    expect(clampSidebarWidth(260)).toBe(260);
    // A drag runs well past the edge of the handle; the width follows the
    // pointer only as far as it is allowed to.
    expect(clampSidebarWidth(1200)).toBe(SIDEBAR_MAX_WIDTH);
    expect(clampSidebarWidth(-40)).toBe(SIDEBAR_MIN_WIDTH);
  });

  it("rounds to whole pixels", () => {
    // Pointer coordinates are fractional on a scaled display, and a fractional
    // column width leaves a seam against the border beside it.
    expect(clampSidebarWidth(240.6)).toBe(241);
  });

  it("falls back to the default for a value that is not a number", () => {
    // Restored from storage written by an older build, or hand-edited.
    expect(clampSidebarWidth(Number.NaN)).toBe(SIDEBAR_DEFAULT_WIDTH);
    expect(clampSidebarWidth(Number.POSITIVE_INFINITY)).toBe(SIDEBAR_DEFAULT_WIDTH);
  });
});
