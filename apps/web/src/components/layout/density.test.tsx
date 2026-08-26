import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { page } from "vitest/browser";

import { AppFrame } from "@/components/layout/app-frame";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { render } from "@/test/render";
import { stubApi } from "@/test/api-stub";

beforeEach(() => stubApi());
afterEach(() => vi.unstubAllGlobals());

const px = (value: string) => Number.parseFloat(value);

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

    // Below 16px iOS Safari zooms the viewport when a field takes focus, so the
    // narrow size has to stay put however tight the wide one gets.
    expect(size("field")).toBe(16);
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

  it("restores touch-sized controls where the pointer is coarse", () => {
    // The runner reports a mouse whatever the viewport is, so the coarse branch
    // cannot be rendered here — but it is the half that keeps the app usable on
    // a phone, and tightening the dense values while forgetting it would be
    // silent. Asserted against the stylesheet instead of the layout.
    const declarations = [...document.styleSheets]
      .flatMap((sheet) => [...sheet.cssRules])
      .filter(
        (rule): rule is CSSMediaRule =>
          rule instanceof CSSMediaRule && rule.conditionText.includes("pointer"),
      )
      .flatMap((rule) => [...rule.cssRules])
      .filter((rule): rule is CSSStyleRule => rule instanceof CSSStyleRule)
      .map((rule) => rule.style.getPropertyValue("--control-height").trim())
      .filter(Boolean);

    expect(declarations).not.toHaveLength(0);
    for (const value of declarations) {
      // Anything under 40px is below what a finger can reliably hit.
      expect(Number.parseFloat(value) * 16).toBeGreaterThanOrEqual(40);
    }
  });
});
