import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { page } from "vitest/browser";

import { AppFrame } from "@/components/layout/app-frame";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { render } from "@/test/render";
import { stubApi } from "@/test/api-stub";

beforeEach(() => stubApi());
afterEach(() => vi.unstubAllGlobals());

const px = (value: string) => Number.parseFloat(value);

/// The custom properties the `(pointer: coarse)` block redefines on `:root`,
/// in px. Read from the stylesheet because the runner always reports a mouse.
function coarseRootTokens() {
  const entries = [...document.styleSheets]
    .flatMap((sheet) => [...sheet.cssRules])
    .filter(
      (rule): rule is CSSMediaRule =>
        rule instanceof CSSMediaRule && rule.conditionText.includes("pointer: coarse"),
    )
    .flatMap((rule) => [...rule.cssRules])
    .filter((rule): rule is CSSStyleRule => rule instanceof CSSStyleRule && rule.selectorText === ":root")
    .flatMap((rule) => [...rule.style].map((name) => [name, rule.style.getPropertyValue(name).trim()]));
  return Object.fromEntries(
    entries.map(([name, value]) => [name, Number.parseFloat(value) * (value.endsWith("rem") ? 16 : 1)]),
  ) as Record<string, number>;
}

function rem(element: Element, property: string) {
  const value = getComputedStyle(element).getPropertyValue(property).trim();
  return Number.parseFloat(value) * (value.endsWith("rem") ? 16 : 1);
}

describe("density", () => {
  it("puts interface text at 13px and captions at 11px", async () => {
    const screen = await render(
      <div>
        <p data-testid="ui" className="text-sm">
          Interface
        </p>
        <p data-testid="caption" className="text-xs">
          Caption
        </p>
        <p data-testid="prose" className="text-prose">
          Prose
        </p>
        <Input data-testid="field" />
      </div>,
    );
    await expect.element(screen.getByTestId("ui")).toBeVisible();

    const size = (id: string) =>
      px(getComputedStyle(screen.getByTestId(id).element()).fontSize);

    expect(size("ui")).toBe(13);
    expect(size("caption")).toBe(11);
    // Prose does not follow the interface down: a body paragraph is read, not
    // scanned, so it keeps its own size and leading.
    expect(size("prose")).toBe(14);
    expect(px(getComputedStyle(screen.getByTestId("prose").element()).lineHeight)).toBe(24);

    // A field is interface text like everything else *where there is a mouse*.
    // The 16px it needs to keep iOS Safari from zooming is keyed to the pointer,
    // not to the viewport, and is asserted in the coarse test below.
    expect(size("field")).toBe(13);
  });

  it("sizes rows from the shared control height, not from per-component literals", async () => {
    await page.viewport(1280, 800);
    const screen = await render(
      <AppFrame>
        <div className="flex gap-2 p-2">
          <Button>Action</Button>
          <Select data-testid="select" className="w-40">
            <option>Watching</option>
          </Select>
        </div>
      </AppFrame>,
    );
    const nav = screen.getByRole("link", { name: "Home" });
    await expect.element(nav).toBeVisible();

    const height = (element: Element) => Math.round(element.getBoundingClientRect().height);
    const control = rem(document.documentElement, "--control-height");

    // 22–28px is the band a desktop app's rows sit in; the point of a single
    // token is that a button, a field and a navigation row cannot disagree.
    expect(control).toBeGreaterThanOrEqual(22);
    expect(control).toBeLessThanOrEqual(28);
    expect(height(screen.getByRole("button", { name: "Action" }).element())).toBe(control);
    expect(height(screen.getByTestId("select").element())).toBe(control);
    expect(height(nav.element())).toBe(control);
    // The toolbar's own field is on the same token, so the chrome and the
    // content it frames cannot end up on different scales.
    expect(height(screen.getByRole("combobox", { name: "Search library" }).element())).toBe(control);
  });

  it("restores touch-sized controls and type where the pointer is coarse", () => {
    // The runner reports a mouse whatever the viewport is, so the coarse branch
    // cannot be rendered here — but it is the half that keeps the app usable on
    // a phone, and tightening the dense values while forgetting it would be
    // silent. Asserted against the stylesheet instead of the layout.
    const coarse = coarseRootTokens();
    expect(Object.keys(coarse)).not.toHaveLength(0);

    // Anything under 40px is below what a finger can reliably hit.
    expect(coarse["--control-height"]).toBeGreaterThanOrEqual(40);

    // 13px interface text is a desktop affordance; at arm's length on a phone it
    // reads as a footnote. Every rung of the scale comes back up.
    expect(coarse["--text-xs"]).toBeGreaterThan(11);
    expect(coarse["--text-sm"]).toBeGreaterThan(13);
    expect(coarse["--text-prose"]).toBeGreaterThan(14);
    expect(coarse["--text-code"]).toBeGreaterThan(11);

    // The zoom floor. iOS Safari zooms the viewport when a focused field is
    // under 16px, so every size a field can take on a touch device has to clear
    // it: `--text-base` is what the `pointer-coarse:text-base` guard resolves
    // to, and prose/code are what the textareas carrying no guard resolve to.
    expect(coarse["--text-base"]).toBeGreaterThanOrEqual(16);
    expect(coarse["--text-prose"]).toBeGreaterThanOrEqual(16);
    expect(coarse["--text-code"]).toBeGreaterThanOrEqual(16);
  });

  it("guards every field against the zoom floor, or leaves it above one", async () => {
    // The guard only works if it is actually on the field. A field that renders
    // at the dense size with no `pointer-coarse:` escape hatch, and whose own
    // size sits under 16px, would zoom the viewport on focus.
    const screen = await render(
      <div>
        <Input data-testid="input" />
        <Textarea data-testid="textarea" />
        <Select data-testid="select">
          <option>Watching</option>
        </Select>
      </div>,
    );
    await expect.element(screen.getByTestId("input")).toBeVisible();

    const coarse = coarseRootTokens();
    for (const id of ["input", "textarea", "select"]) {
      const element = screen.getByTestId(id).element();
      const guarded = element.className.includes("pointer-coarse:text-base");
      const own = px(getComputedStyle(element).fontSize);
      // Either the field carries the guard, or its own token is already at or
      // above the floor once the coarse scale applies.
      expect(guarded || (own === 14 && coarse["--text-prose"] >= 16)).toBe(true);
    }
  });
});
