//! Keyboard chords, and how to write them down.
//!
//! One modifier is spelled `mod` throughout: ⌘ where the platform expects ⌘ and
//! Ctrl everywhere else. Binding sites say what they mean ("mod+K") and never
//! branch on the platform themselves, so a chord and the label the user reads
//! for it can never disagree.

/** A single key combination. `key` is matched against `KeyboardEvent.key`. */
export type Chord = {
  key: string;
  /** ⌘ on Apple keyboards, Ctrl elsewhere. */
  mod?: boolean;
  shift?: boolean;
};

/**
 * Whether ⌘ is the platform's primary modifier. Takes the platform string so
 * the formatting and matching below stay testable on any host; the default
 * reads the running one.
 *
 * `navigator.platform` is deprecated but is the only value WKWebView reports
 * accurately for this (it says `MacIntel` on every Mac, Apple Silicon
 * included), and `lib/desktop.ts` already reads it for the same reason.
 */
export function isAppleKeyboard(platform: string = navigator.platform): boolean {
  return /^(Mac|iPhone|iPad|iPod)/.test(platform);
}

/**
 * Whether a keyboard event is exactly this chord.
 *
 * "Exactly" is the point: a chord without `shift` must not fire when Shift is
 * held, or ⌘1 would also trigger on ⌘! — and the non-primary modifier is
 * rejected outright so ⌃⌘K on a Mac is not read as ⌘K.
 */
export function matchesChord(
  event: Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">,
  chord: Chord,
  apple: boolean = isAppleKeyboard(),
): boolean {
  if (event.key.toLowerCase() !== chord.key.toLowerCase()) return false;
  if (event.altKey) return false;
  if (event.shiftKey !== Boolean(chord.shift)) return false;
  const primary = apple ? event.metaKey : event.ctrlKey;
  const secondary = apple ? event.ctrlKey : event.metaKey;
  if (secondary) return false;
  return primary === Boolean(chord.mod);
}

/**
 * The chord as a user reads it: `⌘K` on Apple, `Ctrl+K` elsewhere — each
 * platform's own convention, symbols joined and words plus-separated.
 *
 * Not translated: these are the symbols printed on the keys, and every locale's
 * software writes them the same way.
 */
export function formatChord(chord: Chord, apple: boolean = isAppleKeyboard()): string {
  // The two platforms order their modifiers differently: macOS prints them
  // ⌃⌥⇧⌘ so Shift comes *before* Command, while Windows and Linux lead with
  // Ctrl. Following each convention is most of what makes the hint look like it
  // belongs to the OS rather than to a web page.
  const parts: string[] = apple
    ? [...(chord.shift ? ["⇧"] : []), ...(chord.mod ? ["⌘"] : [])]
    : [...(chord.mod ? ["Ctrl"] : []), ...(chord.shift ? ["Shift"] : [])];
  parts.push(displayKey(chord.key));
  return apple ? parts.join("") : parts.join("+");
}

function displayKey(key: string) {
  if (key === "ArrowLeft") return "←";
  if (key === "ArrowRight") return "→";
  // Single letters are printed uppercase on the key itself; punctuation and
  // digits are already what the key says.
  return key.length === 1 ? key.toUpperCase() : key;
}

// MARK: The app's chords.
//
// Kept together so the set can be read at a glance and a new binding is
// obviously a new entry rather than a stray `metaKey` check somewhere.

/** Open the command palette. */
export const paletteChord: Chord = { key: "k", mod: true };
/** Focus the library search field. */
export const searchChord: Chord = { key: "f", mod: true };
/** Show or hide the sidebar. */
export const sidebarChord: Chord = { key: "\\", mod: true };
/** Open settings. */
export const settingsChord: Chord = { key: ",", mod: true };
/** Step back and forward through the in-app history. */
export const backChord: Chord = { key: "[", mod: true };
export const forwardChord: Chord = { key: "]", mod: true };

/**
 * `mod+1` … `mod+9` for the first nine navigation destinations, by position.
 *
 * Bound in the desktop shell only. In a browser these switch tabs, and taking
 * that over costs the user more than the accelerator gives them back — the
 * palette reaches the same destinations on every platform.
 */
export function destinationChord(index: number): Chord | undefined {
  return index < 9 ? { key: String(index + 1), mod: true } : undefined;
}
