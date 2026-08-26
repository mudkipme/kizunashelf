import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { page, userEvent } from "vitest/browser";

import { AppFrame } from "@/components/layout/app-frame";
import { isAppleKeyboard } from "@/lib/shortcuts";
import {
  SIDEBAR_DEFAULT_WIDTH,
  clampSidebarWidth,
  SIDEBAR_MAX_WIDTH,
  SIDEBAR_MIN_WIDTH,
  sidebarStorageKey,
  useSidebarStore,
} from "@/lib/sidebar";
import { render } from "@/test/render";
import { stubApi, testStats } from "@/test/api-stub";

const mod = isAppleKeyboard() ? "Meta" : "Control";

beforeEach(() => {
  stubApi();
  // The store persists to localStorage, so it survives between tests in the
  // same page unless it is put back.
  useSidebarStore.setState({ width: SIDEBAR_DEFAULT_WIDTH, collapsed: false });
});
afterEach(() => {
  vi.unstubAllGlobals();
  useSidebarStore.setState({ width: SIDEBAR_DEFAULT_WIDTH, collapsed: false });
});

const sidebar = () => document.querySelector("aside.app-chrome");
const width = () => useSidebarStore.getState().width;

/** Elements actually painted — the brand exists twice in the DOM, once per breakpoint. */
function visibleAppNames() {
  return [...document.querySelectorAll("*")].filter(
    (element) =>
      element.childElementCount === 0 &&
      element.textContent === "KizunaShelf" &&
      element.checkVisibility(),
  );
}

describe("sidebar", () => {
  it("collapses from the header control and comes back", async () => {
    await page.viewport(1280, 800);
    const screen = await render(<AppFrame>{null}</AppFrame>);
    await expect.element(screen.getByRole("link", { name: "Home" })).toBeVisible();

    await screen.getByRole("button", { name: "Hide sidebar" }).click();
    await expect.poll(sidebar).toBe(null);

    // The control has to survive the collapse, or there is no way back.
    await screen.getByRole("button", { name: "Show sidebar" }).click();
    await expect.poll(() => sidebar() !== null).toBe(true);
  });

  it("collapses from its chord", async () => {
    await page.viewport(1280, 800);
    const screen = await render(<AppFrame>{null}</AppFrame>);
    await expect.element(screen.getByRole("link", { name: "Home" })).toBeVisible();

    await userEvent.keyboard(`{${mod}>}\\{/${mod}}`);
    await expect.poll(sidebar).toBe(null);
    await userEvent.keyboard(`{${mod}>}\\{/${mod}}`);
    await expect.poll(() => sidebar() !== null).toBe(true);
  });

  it("keeps the layout the user chose for the next launch", async () => {
    await page.viewport(1280, 800);
    const screen = await render(<AppFrame>{null}</AppFrame>);
    const handle = screen.getByRole("separator", { name: "Resize sidebar" });

    handle.element().focus();
    await userEvent.keyboard("{ArrowRight}");
    await screen.getByRole("button", { name: "Hide sidebar" }).click();
    await expect.poll(sidebar).toBe(null);

    // Asserted on the stored value rather than by remounting: a second render
    // inside one test would mount a second shell beside the first, and tearing
    // the first one down by hand leaves the harness unable to mount again.
    await expect
      .poll(() => JSON.parse(localStorage.getItem(sidebarStorageKey) ?? "{}").state)
      .toEqual({ width: SIDEBAR_DEFAULT_WIDTH + 16, collapsed: true });
  });

  it("widens and narrows by dragging the separator", async () => {
    await page.viewport(1280, 800);
    const screen = await render(<AppFrame>{null}</AppFrame>);
    const handle = screen.getByRole("separator", { name: "Resize sidebar" });
    await expect.element(handle).toBeInTheDocument();

    // Dragged far past the maximum on purpose: the pointer is allowed to run
    // anywhere, the column is not.
    await userEvent.dragAndDrop(handle, screen.getByRole("button", { name: "Rescan vault" }));
    await expect.poll(width).toBe(SIDEBAR_MAX_WIDTH);
    expect(sidebar()?.getBoundingClientRect().width).toBeCloseTo(SIDEBAR_MAX_WIDTH, 0);

    // Dragged back onto something *inside* the widened sidebar. The header's
    // own controls are no use as a target here: they move right along with the
    // column, so they are never to the handle's left.
    const home = screen.getByRole("link", { name: "Home" });
    const drop = home.element().getBoundingClientRect();
    // The drag lands on the target's centre, and the handle straddles the edge
    // it moves — so the new width is simply where the pointer was let go.
    const dropX = Math.round(drop.left + drop.width / 2);
    await userEvent.dragAndDrop(handle, home);
    await expect.poll(width).toBe(clampSidebarWidth(dropX));
    expect(width()).toBeLessThan(SIDEBAR_MAX_WIDTH);
  });

  it("resizes with the keyboard alone", async () => {
    await page.viewport(1280, 800);
    const screen = await render(<AppFrame>{null}</AppFrame>);
    const handle = screen.getByRole("separator", { name: "Resize sidebar" });

    // A pointer-only handle would put the width out of reach entirely.
    handle.element().focus();
    await userEvent.keyboard("{ArrowRight}");
    await expect.poll(width).toBe(SIDEBAR_DEFAULT_WIDTH + 16);
    await userEvent.keyboard("{ArrowLeft}{ArrowLeft}");
    await expect.poll(width).toBe(SIDEBAR_DEFAULT_WIDTH - 16);

    await userEvent.keyboard("{End}");
    await expect.poll(width).toBe(SIDEBAR_MAX_WIDTH);
    await userEvent.keyboard("{Home}");
    await expect.poll(width).toBe(SIDEBAR_MIN_WIDTH);

    // The way back from an awkward drag.
    await userEvent.keyboard("{Enter}");
    await expect.poll(width).toBe(SIDEBAR_DEFAULT_WIDTH);
  });

  it("reports the width it is on, for anyone not looking at it", async () => {
    await page.viewport(1280, 800);
    const screen = await render(<AppFrame>{null}</AppFrame>);
    const handle = screen.getByRole("separator", { name: "Resize sidebar" });

    await expect.element(handle).toHaveAttribute("aria-valuenow", String(SIDEBAR_DEFAULT_WIDTH));
    await expect.element(handle).toHaveAttribute("aria-valuemin", String(SIDEBAR_MIN_WIDTH));
    await expect.element(handle).toHaveAttribute("aria-valuemax", String(SIDEBAR_MAX_WIDTH));
  });

  it("indents every row's label alike, whatever kind of glyph it carries", async () => {
    // Lucide icons carry a 24px intrinsic size. `Button` normalises that, a
    // `NavLink` does not — so an icon row pushed its label 8px further right
    // than an emoji row, and a 24px glyph crowded a 28px row beside 13px text.
    stubApi({
      stats: {
        ...testStats,
        byType: [
          { id: "anime", label: "Anime", count: 2 },
          { id: "manga", label: "Manga", icon: "📚", count: 7 },
        ],
      },
    });
    await page.viewport(1280, 800);
    const screen = await render(<AppFrame>{null}</AppFrame>);
    await expect.element(screen.getByRole("link", { name: /Manga/ })).toBeVisible();

    // Nav rows only — the brand link above them carries a larger mark.
    const rows = [...(sidebar()?.querySelectorAll("section a") ?? [])];
    for (const row of rows) {
      expect(row.firstElementChild!.getBoundingClientRect().height).toBe(16);
    }

    const labelLeft = (text: string) =>
      rows
        // `includes`, not `startsWith`: an emoji row leads with its glyph.
        .find((row) => row.textContent?.includes(text))!
        .querySelector("span:nth-child(2)")!
        .getBoundingClientRect().left;
    expect(labelLeft("Manga")).toBe(labelLeft("Anime"));
  });
});

describe("app name", () => {
  // The point of moving the brand into the sidebar: one layout, one place the
  // app says its own name — never the two it used to show at once.
  it("appears exactly once with the sidebar beside the content", async () => {
    await page.viewport(1280, 800);
    const screen = await render(<AppFrame>{null}</AppFrame>);
    await expect.element(screen.getByRole("link", { name: "Home" })).toBeVisible();

    expect(visibleAppNames()).toHaveLength(1);
    // ...and it is the sidebar's, not the header's.
    expect(sidebar()?.contains(visibleAppNames()[0])).toBe(true);
  });

  it("comes from the navigation sheet where there is no sidebar", async () => {
    await page.viewport(600, 800);
    const screen = await render(<AppFrame>{null}</AppFrame>);
    const openNavigation = screen.getByRole("button", { name: "Open navigation" });
    await expect.element(openNavigation).toBeVisible();

    // The bar carries no brand at all: that width belongs to the history
    // controls, which an installed app has no browser chrome to replace.
    expect(sidebar()?.checkVisibility()).not.toBe(true);
    expect(visibleAppNames()).toHaveLength(0);

    await openNavigation.click();
    await expect.element(screen.getByRole("dialog")).toBeVisible();
    expect(visibleAppNames()).toHaveLength(1);
  });
});
