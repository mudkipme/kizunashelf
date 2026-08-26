import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { page } from "vitest/browser";

import { AppFrame } from "@/components/layout/app-frame";
import { render } from "@/test/render";
import type { StatsResponse } from "@/types/api";

const stats: StatsResponse = {
  generatedAt: "2026-01-01T00:00:00Z",
  total: 2,
  relations: 0,
  byType: [{ id: "anime", label: "Anime", count: 2 }],
  byCanonicalStatus: { planning: 0, ongoing: 1, paused: 0, completed: 1, dropped: 0 },
  dateFields: [],
};

// The shell fetches its sidebar counts on mount. Served from here so the test
// depends on the contract rather than on whatever the runner's dev server
// happens to do with an unproxied `/api` path.
beforeEach(() => {
  vi.stubGlobal("fetch", async () =>
    new Response(JSON.stringify(stats), { headers: { "content-type": "application/json" } }),
  );
});
afterEach(() => {
  vi.unstubAllGlobals();
});

/**
 * The "chrome is not a document" rules live in `index.css` and are applied by
 * class, so what is worth pinning is the *result* on the real shell: which
 * elements ended up marked as chrome and which did not.
 *
 * The direction that matters is content staying selectable. Turning selection
 * off is a one-line rule anyone can widen by accident, and the failure mode —
 * text that silently cannot be copied — is invisible until a user hits it.
 */
function styleOf(selector: string) {
  const element = document.querySelector(selector);
  if (!element) throw new Error(`no element matched ${selector}`);
  return getComputedStyle(element);
}

describe("AppFrame chrome", () => {
  it("marks the shell as chrome without taking selection from content", async () => {
    // Wide enough for the sidebar, which is `hidden md:flex`.
    await page.viewport(1280, 800);
    const screen = await render(
      <AppFrame>
        <p data-testid="content">Frieren at the Funeral</p>
      </AppFrame>,
    );
    await expect.element(screen.getByRole("link", { name: "Library" })).toBeVisible();

    expect(styleOf("header.app-chrome").getPropertyValue("user-select")).toBe("none");
    expect(styleOf("aside.app-chrome").getPropertyValue("user-select")).toBe("none");

    // Anything the app renders as data — an entity page, a diagnostic, a value
    // returned by a provider — is still ordinary selectable text.
    expect(styleOf('[data-testid="content"]').getPropertyValue("user-select")).not.toBe("none");
  });

  it("keeps a field inside the chrome editable and copyable", async () => {
    await page.viewport(1280, 800);
    await render(<AppFrame>{null}</AppFrame>);

    // The header search sits inside a region with selection switched off, so
    // the input has to opt back in or its own text becomes unselectable.
    expect(styleOf("header.app-chrome input").getPropertyValue("user-select")).toBe("text");
  });

  it("uses the arrow cursor over navigation, not the hyperlink hand", async () => {
    await page.viewport(1280, 800);
    await render(<AppFrame>{null}</AppFrame>);

    expect(styleOf("aside.app-chrome a").getPropertyValue("cursor")).toBe("default");
    expect(styleOf('[data-slot="button"]').getPropertyValue("cursor")).toBe("default");
  });

  it("does not let an image start a drag", async () => {
    await render(<AppFrame>{null}</AppFrame>);

    // Otherwise dragging a cover starts an HTML5 drag with a ghost image that
    // nothing in the app accepts.
    expect(styleOf("img").getPropertyValue("-webkit-user-drag")).toBe("none");
  });
});
