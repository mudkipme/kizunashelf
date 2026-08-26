import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import { useLocation } from "react-router-dom";

import { AppFrame } from "@/components/layout/app-frame";
import { isAppleKeyboard } from "@/lib/shortcuts";
import { render } from "@/test/render";
import { stubApi } from "@/test/api-stub";

const mod = isAppleKeyboard() ? "Meta" : "Control";
const chord = (key: string) => `{${mod}>}${key.replace(/[[{]/g, "$&$&")}{/${mod}}`;

beforeEach(() => stubApi());
afterEach(() => vi.unstubAllGlobals());

function Probe() {
  return <span data-testid="path">{useLocation().pathname}</span>;
}

const shell = () =>
  render(
    <AppFrame>
      <Probe />
    </AppFrame>,
  );

describe("navigation controls", () => {
  it("enables forward only once there is somewhere to go forward to", async () => {
    await page.viewport(1280, 800);
    const screen = await shell();
    const back = screen.getByRole("button", { name: "Go back" });
    const forward = screen.getByRole("button", { name: "Go forward" });

    // Nothing behind and nothing ahead: both read as unavailable rather than
    // being present and inert.
    await expect.element(back).toBeDisabled();
    await expect.element(forward).toBeDisabled();

    await screen.getByRole("link", { name: "Calendar" }).click();
    await expect.element(screen.getByTestId("path")).toHaveTextContent("/calendar");
    await expect.element(back).not.toBeDisabled();
    await expect.element(forward).toBeDisabled();

    await back.click();
    await expect.element(screen.getByTestId("path")).toHaveTextContent("/");
    await expect.element(forward).not.toBeDisabled();

    await forward.click();
    await expect.element(screen.getByTestId("path")).toHaveTextContent("/calendar");
    await expect.element(forward).toBeDisabled();
  });

  it("drops the forward entries when a new page is opened from further back", async () => {
    await page.viewport(1280, 800);
    const screen = await shell();

    await screen.getByRole("link", { name: "Calendar" }).click();
    await expect.element(screen.getByTestId("path")).toHaveTextContent("/calendar");
    await userEvent.keyboard(chord("["));
    await expect.element(screen.getByRole("button", { name: "Go forward" })).not.toBeDisabled();

    // Navigating somewhere new from a rewound position discards what was ahead,
    // exactly as the browser's own history does.
    await screen.getByRole("link", { name: "Lists" }).click();
    await expect.element(screen.getByTestId("path")).toHaveTextContent("/lists");
    await expect.element(screen.getByRole("button", { name: "Go forward" })).toBeDisabled();
    await expect.element(screen.getByRole("button", { name: "Go back" })).not.toBeDisabled();
  });
});

describe("toolbar", () => {
  it("carries only navigation, search and the vault action", async () => {
    await page.viewport(1280, 800);
    const screen = await shell();
    await expect.element(screen.getByRole("button", { name: "Go back" })).toBeVisible();

    const header = document.querySelector("header.app-chrome");
    const labels = [...(header?.querySelectorAll("button") ?? [])].map((button) =>
      button.getAttribute("aria-label"),
    );

    // Theme and language moved to Settings — they are preferences, not actions,
    // and every icon left in the toolbar is noise above the content.
    expect(labels).toEqual([
      "Hide sidebar",
      "Open navigation",
      "Go back",
      "Go forward",
      // The narrow-viewport search toggle; hidden from the sm breakpoint up.
      "Search library",
      "Rescan vault",
    ]);
  });

  it("still names the destination once the page stops repeating it", async () => {
    await page.viewport(1280, 800);
    const screen = await shell();

    await screen.getByRole("link", { name: "Calendar" }).click();
    // With the title gone from the page body, the window/tab title is what says
    // where you are — which also covers a collapsed sidebar.
    await expect.poll(() => document.title).toBe("Calendar — KizunaShelf");
  });
});
