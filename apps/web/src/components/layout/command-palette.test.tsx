import { useLocation } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { vi } from "vitest";
import { page, userEvent } from "vitest/browser";

import { AppFrame } from "@/components/layout/app-frame";
import { isAppleKeyboard } from "@/lib/shortcuts";
import { stubApi, testEntity } from "@/test/api-stub";
import { render } from "@/test/render";

// The chords are written `mod+…`, so the tests press whatever this host's `mod`
// is. That keeps the suite honest on a Mac and on CI both.
const mod = isAppleKeyboard() ? "Meta" : "Control";
// `[` and `{` open the keyboard DSL's own descriptors, so a literal one has to
// be doubled.
const chord = (key: string) => `{${mod}>}${key.replace(/[[{]/g, "$&$&")}{/${mod}}`;

const PALETTE = "Search entities, pages and commands";

beforeEach(() => {
  stubApi({ entities: [testEntity({ id: "anime/Frieren", title: "Frieren" })] });
});
afterEach(() => {
  vi.unstubAllGlobals();
});

/** Reports the router's current location so navigation can be asserted. */
function Probe() {
  const location = useLocation();
  return <span data-testid="path">{`${location.pathname}${location.search}`}</span>;
}

function shell(route = "/") {
  return render(
    <AppFrame>
      <Probe />
    </AppFrame>,
    { route },
  );
}

describe("command palette", () => {
  it("opens on its chord and closes on Escape", async () => {
    await page.viewport(1280, 800);
    const screen = await shell();

    expect(screen.getByRole("listbox").elements()).toHaveLength(0);

    await userEvent.keyboard(chord("k"));

    // The field takes focus on open: a palette you have to click into first is
    // not a keyboard affordance at all.
    await expect.element(screen.getByRole("combobox", { name: PALETTE })).toHaveFocus();

    await userEvent.keyboard("{Escape}");
    await expect.poll(() => screen.getByRole("listbox").elements().length).toBe(0);
  });

  it("opens from inside a text field", async () => {
    await page.viewport(1280, 800);
    const screen = await shell();

    await screen.getByRole("combobox", { name: "Search library" }).click();
    await userEvent.keyboard("fri");
    // Every chord carries the platform modifier, so typing is never a reason to
    // swallow one — this is how the palette is reached in every app that has one.
    await userEvent.keyboard(chord("k"));

    await expect.element(screen.getByRole("combobox", { name: PALETTE })).toHaveFocus();
  });

  it("filters to a destination and opens it on Enter", async () => {
    await page.viewport(1280, 800);
    const screen = await shell();
    await userEvent.keyboard(chord("k"));

    await userEvent.keyboard("calen");
    await expect
      .poll(() =>
        screen
          .getByRole("option")
          .elements()
          .map((option) => option.textContent),
      )
      .toEqual(["Calendar"]);

    await userEvent.keyboard("{Enter}");

    await expect.element(screen.getByTestId("path")).toHaveTextContent("/calendar");
    // Opening something is the end of the palette's job.
    expect(screen.getByRole("listbox").elements()).toHaveLength(0);
  });

  it("reaches an entity by its title", async () => {
    await page.viewport(1280, 800);
    const screen = await shell();
    await userEvent.keyboard(chord("k"));
    await userEvent.keyboard("frieren");

    // Entities lead the list once there is a query, so Enter goes straight
    // there without arrowing past the navigation matches.
    await expect.element(screen.getByRole("option").first()).toHaveTextContent("Frieren");
    await userEvent.keyboard("{Enter}");

    await expect.element(screen.getByTestId("path")).toHaveTextContent("/entities/anime%2FFrieren");
  });

  it("offers the vault's own types alongside the fixed destinations", async () => {
    await page.viewport(1280, 800);
    const screen = await shell();
    await userEvent.keyboard(chord("k"));

    // "Anime" is a type from the vault schema, not a route the app ships.
    await expect.element(screen.getByRole("option", { name: "Anime" })).toBeVisible();
    await userEvent.keyboard("{Enter}{Escape}");
  });

  it("wraps the highlight around the ends of the list", async () => {
    await page.viewport(1280, 800);
    const screen = await shell();
    await userEvent.keyboard(chord("k"));

    const selected = () =>
      screen
        .getByRole("option")
        .elements()
        .find((option) => option.getAttribute("aria-selected") === "true")?.textContent;

    await expect.poll(selected).toBe("Home");
    // One press up from the top is the fastest way to the last action.
    await userEvent.keyboard("{ArrowUp}");
    await expect.poll(selected).toBe("Rescan vault");
    await userEvent.keyboard("{ArrowDown}");
    await expect.poll(selected).toBe("Home");
  });
});

describe("app shortcuts", () => {
  it("leaves mod+F to the browser's find", async () => {
    // Search lives in the palette (mod+K); taking over mod+F would cost the user
    // find-in-page on long notes for no gain.
    await page.viewport(1280, 800);
    const screen = await shell("/library?q=frieren");
    const search = screen.getByRole("combobox", { name: "Search library" });

    const event = new KeyboardEvent("keydown", {
      key: "f",
      metaKey: isAppleKeyboard(),
      ctrlKey: !isAppleKeyboard(),
      bubbles: true,
      cancelable: true,
    });
    window.dispatchEvent(event);

    expect(event.defaultPrevented).toBe(false);
    await expect.element(search).not.toHaveFocus();
  });

  it("steps back and forward through the in-app history", async () => {
    await page.viewport(1280, 800);
    const screen = await shell();

    await userEvent.keyboard(chord(","));
    await expect.element(screen.getByTestId("path")).toHaveTextContent("/settings");

    // The desktop shell has no browser chrome to go back with, so the app has
    // to carry its own history keys.
    await userEvent.keyboard(chord("["));
    await expect.poll(() => screen.getByTestId("path").element().textContent).toBe("/");

    await userEvent.keyboard(chord("]"));
    await expect.element(screen.getByTestId("path")).toHaveTextContent("/settings");
  });

  it("leaves the browser's tab shortcuts alone outside the desktop shell", async () => {
    await page.viewport(1280, 800);
    const screen = await shell();

    await userEvent.keyboard(chord("2"));

    // mod+2 is Library in the desktop shell. In a browser it switches tabs, and
    // the palette already reaches every destination — so nothing is bound here.
    await expect.element(screen.getByTestId("path")).toHaveTextContent("/");
  });
});
