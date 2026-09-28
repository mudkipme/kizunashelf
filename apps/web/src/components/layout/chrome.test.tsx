import { useLocation, useNavigate } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { page, userEvent } from "vitest/browser";

import { AppFrame } from "@/components/layout/app-frame";
import { isAppleKeyboard } from "@/lib/shortcuts";
import { stubApi } from "@/test/api-stub";
import { render } from "@/test/render";

const mod = isAppleKeyboard() ? "Meta" : "Control";
const chord = (key: string) => `{${mod}>}${key.replace(/[[{]/g, "$&$&")}{/${mod}}`;

beforeEach(() => stubApi());
afterEach(() => vi.unstubAllGlobals());

function Probe() {
  return <span data-testid="path">{useLocation().pathname}</span>;
}

function RoutedShell() {
  const location = useLocation();
  const navigate = useNavigate();
  // Each real route owns its AppFrame, so page changes remount the toolbar.
  return (
    <AppFrame key={location.pathname}>
      <Probe />
      <button onClick={() => navigate("/activity", { replace: true })}>Replace page</button>
    </AppFrame>
  );
}

const shell = () => render(<RoutedShell />);

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

  it("preserves forward history when the current page is replaced", async () => {
    await page.viewport(1280, 800);
    const screen = await shell();
    const back = screen.getByRole("button", { name: "Go back" });
    const forward = screen.getByRole("button", { name: "Go forward" });

    await screen.getByRole("link", { name: "Calendar" }).click();
    await screen.getByRole("link", { name: "Lists" }).click();
    await back.click();
    await expect.element(screen.getByTestId("path")).toHaveTextContent("/calendar");
    await screen.getByRole("button", { name: "Replace page" }).click();
    await expect.element(screen.getByTestId("path")).toHaveTextContent("/activity");
    await expect.element(forward).not.toBeDisabled();

    await forward.click();
    await expect.element(screen.getByTestId("path")).toHaveTextContent("/lists");
    await expect.element(forward).toBeDisabled();
    await back.click();
    await expect.element(screen.getByTestId("path")).toHaveTextContent("/activity");
    await back.click();
    await expect.element(screen.getByTestId("path")).toHaveTextContent("/");
    await expect.element(back).toBeDisabled();
  });
});

describe("toolbar", () => {
  it("fills the narrow bar instead of leaving a gap at its end", async () => {
    // The trailing group is what holds the bar's right edge. When it held only
    // the rescan button — which is itself hidden on a phone — it rendered
    // empty, and every control bunched up against the left with dead space
    // beside it.
    await page.viewport(390, 720);
    const screen = await shell();
    await expect.element(screen.getByRole("button", { name: "Open navigation" })).toBeVisible();

    await expect.element(screen.getByRole("button", { name: "Language" })).toBeVisible();
    const header = document.querySelector("header.app-chrome") as HTMLElement;
    const trailing = header.lastElementChild as HTMLElement;
    expect([...trailing.children].filter((child) => child.checkVisibility())).not.toHaveLength(0);

    // Added to a home screen the app runs with no browser chrome, so these are
    // the only way to move through history — they have to survive the narrowest
    // width, which is what taking the brand out of the bar paid for.
    await expect.element(screen.getByRole("button", { name: "Go back" })).toBeVisible();
    await expect.element(screen.getByRole("button", { name: "Go forward" })).toBeVisible();

    // Nothing spills out of the bar at the narrowest width worth supporting.
    await page.viewport(320, 640);
    await expect.poll(() => header.scrollWidth <= header.clientWidth).toBe(true);
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
