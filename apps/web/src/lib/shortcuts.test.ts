import { describe, expect, it } from "vitest";

import { formatChord, isAppleKeyboard, matchesChord } from "@/lib/shortcuts";

function press(overrides: Partial<KeyboardEvent> & Pick<KeyboardEvent, "key">) {
  return {
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    ...overrides,
  } as KeyboardEvent;
}

describe("isAppleKeyboard", () => {
  it("recognizes the platforms that use ⌘", () => {
    // WKWebView says `MacIntel` on every Mac, Apple Silicon included.
    expect(isAppleKeyboard("MacIntel")).toBe(true);
    expect(isAppleKeyboard("iPhone")).toBe(true);
    expect(isAppleKeyboard("Win32")).toBe(false);
    expect(isAppleKeyboard("Linux x86_64")).toBe(false);
  });
});

describe("matchesChord", () => {
  it("reads `mod` as ⌘ on Apple and Ctrl elsewhere", () => {
    const chord = { key: "k", mod: true };
    expect(matchesChord(press({ key: "k", metaKey: true }), chord, true)).toBe(true);
    expect(matchesChord(press({ key: "k", ctrlKey: true }), chord, true)).toBe(false);

    expect(matchesChord(press({ key: "k", ctrlKey: true }), chord, false)).toBe(true);
    expect(matchesChord(press({ key: "k", metaKey: true }), chord, false)).toBe(false);
  });

  it("refuses a chord carrying modifiers it did not ask for", () => {
    const chord = { key: "1", mod: true };
    // ⇧⌘1 is a different chord, and on many layouts it does not even produce
    // "1" — matching it here would fire navigation on a Shift-digit symbol.
    expect(matchesChord(press({ key: "1", metaKey: true, shiftKey: true }), chord, true)).toBe(false);
    expect(matchesChord(press({ key: "1", metaKey: true, altKey: true }), chord, true)).toBe(false);
    // ⌃⌘1 holds the non-primary modifier too, so it is not ⌘1.
    expect(matchesChord(press({ key: "1", metaKey: true, ctrlKey: true }), chord, true)).toBe(false);
    expect(matchesChord(press({ key: "1", metaKey: true }), chord, true)).toBe(true);
  });

  it("requires Shift when the chord asks for it", () => {
    const chord = { key: "p", mod: true, shift: true };
    expect(matchesChord(press({ key: "P", metaKey: true, shiftKey: true }), chord, true)).toBe(true);
    expect(matchesChord(press({ key: "p", metaKey: true }), chord, true)).toBe(false);
  });

  it("ignores the case the layout reports", () => {
    expect(matchesChord(press({ key: "K", metaKey: true }), { key: "k", mod: true }, true)).toBe(true);
  });

  it("does not fire an unmodified chord's key when the modifier is held", () => {
    expect(matchesChord(press({ key: "k", metaKey: true }), { key: "k" }, true)).toBe(false);
  });
});

describe("formatChord", () => {
  it("writes each platform's own convention", () => {
    expect(formatChord({ key: "k", mod: true }, true)).toBe("⌘K");
    expect(formatChord({ key: "k", mod: true }, false)).toBe("Ctrl+K");
    expect(formatChord({ key: "p", mod: true, shift: true }, true)).toBe("⇧⌘P");
    expect(formatChord({ key: "p", mod: true, shift: true }, false)).toBe("Ctrl+Shift+P");
  });

  it("prints punctuation and digits as the key itself", () => {
    expect(formatChord({ key: ",", mod: true }, true)).toBe("⌘,");
    expect(formatChord({ key: "1", mod: true }, true)).toBe("⌘1");
    expect(formatChord({ key: "[", mod: true }, true)).toBe("⌘[");
  });
});
