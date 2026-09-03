import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { page } from "vitest/browser";

import { AppFrame } from "@/components/layout/app-frame";
import { SIDEBAR_DEFAULT_WIDTH, useSidebarStore } from "@/lib/sidebar";
import { stubApi } from "@/test/api-stub";
import { render } from "@/test/render";

// The only macOS-specific input the header reads. Forced on so the branch that
// reserves the traffic lights' corner can be exercised off a Mac.
vi.mock("@/hooks/use-mac-titlebar-inset", () => ({ useMacTitlebarInset: () => true }));

/// Roughly where the rightmost traffic light ends; nothing interactive may
/// start left of this while they are on screen.
const TRAFFIC_LIGHTS_WIDTH = 72;

beforeEach(() => {
  stubApi();
  useSidebarStore.setState({ width: SIDEBAR_DEFAULT_WIDTH, collapsed: false });
});
afterEach(() => {
  vi.unstubAllGlobals();
  useSidebarStore.setState({ width: SIDEBAR_DEFAULT_WIDTH, collapsed: false });
});

const firstControlLeft = () =>
  document.querySelector("header.app-chrome button")!.getBoundingClientRect().left;

describe("macOS traffic lights", () => {
  it("keeps the header's leading control clear of them once the sidebar is collapsed", async () => {
    await page.viewport(1280, 800);
    const screen = await render(<AppFrame>{null}</AppFrame>);
    await expect.element(screen.getByRole("button", { name: "Hide sidebar" })).toBeVisible();

    // Expanded, the sidebar owns the corner and has its own drag strip.
    expect(firstControlLeft()).toBeGreaterThanOrEqual(SIDEBAR_DEFAULT_WIDTH);

    await screen.getByRole("button", { name: "Hide sidebar" }).click();
    await expect.element(screen.getByRole("button", { name: "Show sidebar" })).toBeVisible();

    // Collapsed, the header itself has to reserve that corner. Expressed as
    // padding this silently did nothing: `pl-*` loses to the `sm:px-*` on the
    // same element, so the lights sat on top of the toggle and back buttons.
    expect(firstControlLeft()).toBeGreaterThanOrEqual(TRAFFIC_LIGHTS_WIDTH);
  });

  it("leaves the reserved corner draggable", async () => {
    await page.viewport(1280, 800);
    const screen = await render(<AppFrame>{null}</AppFrame>);
    await screen.getByRole("button", { name: "Hide sidebar" }).click();
    await expect.element(screen.getByRole("button", { name: "Show sidebar" })).toBeVisible();

    // The strip beside the lights is part of the title bar, so dragging it
    // should move the window rather than do nothing.
    const spacer = document.querySelector("header.app-chrome > [data-tauri-drag-region]");
    expect(spacer).not.toBe(null);
  });
});
