import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { page } from "vitest/browser";

import { AppFrame } from "@/components/layout/app-frame";
import { useThemeStore } from "@/lib/theme";
import { render } from "@/test/render";
import { stubApi } from "@/test/api-stub";

beforeEach(() => {
  stubApi();
});
afterEach(() => {
  vi.unstubAllGlobals();
  useThemeStore.getState().setMode("system");
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

  // What a native window separates by tone, this app separated only by a
  // hairline: `--color-card` and `--color-background` held the same value in
  // light mode, so the sidebar was literally white on white. The risk in fixing
  // that is a *new* collision — a selected row or a filled button whose token
  // happens to equal the chrome it now sits on, which is exactly what happened
  // to `--color-accent` on the first attempt. Both themes are pinned, one test
  // each so the render between them is torn down.
  for (const mode of ["light", "dark"] as const) {
    it(`keeps chrome, content and selection distinct in ${mode}`, async () => {
      useThemeStore.getState().setMode(mode);
      await page.viewport(1280, 800);
      const screen = await render(<AppFrame>{null}</AppFrame>);
      const home = screen.getByRole("link", { name: "Home" });
      await expect.element(home).toBeVisible();

      const content = styleOf("main").backgroundColor;
      const chrome = styleOf("aside.app-chrome").backgroundColor;
      const selected = getComputedStyle(home.element()).backgroundColor;

      expect(chrome, "the sidebar must not match content").not.toBe(content);
      expect(selected, "selection must not match the sidebar").not.toBe(chrome);
    });
  }

  it("does not let an image start a drag", async () => {
    await render(<AppFrame>{null}</AppFrame>);

    // Otherwise dragging a cover starts an HTML5 drag with a ghost image that
    // nothing in the app accepts.
    expect(styleOf("img").getPropertyValue("-webkit-user-drag")).toBe("none");
  });
});
