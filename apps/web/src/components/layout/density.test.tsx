import { describe, expect, it } from "vitest";

import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { render } from "@/test/render";

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
    .filter(
      (rule): rule is CSSStyleRule => rule instanceof CSSStyleRule && rule.selectorText === ":root",
    )
    .flatMap((rule) =>
      [...rule.style].map((name) => [name, rule.style.getPropertyValue(name).trim()]),
    );
  return Object.fromEntries(
    entries.map(([name, value]) => [
      name,
      Number.parseFloat(value) * (value.endsWith("rem") ? 16 : 1),
    ]),
  ) as Record<string, number>;
}

describe("density", () => {
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
