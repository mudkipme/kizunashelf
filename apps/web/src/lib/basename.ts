const FORBIDDEN_BASENAME_CHARS = new Set(['\\', "/", ":", "*", "?", '"', "<", ">", "|"]);

// Windows reserves these device names regardless of extension, so `CON.md`
// (and `CON.anything.md`) is refused by the OS. Matched against the stem before
// the first dot, case-insensitively.
const RESERVED_WINDOWS_NAME = /^(con|prn|aux|nul|com[0-9]|lpt[0-9])$/i;

export function normalizeBasename(value: string) {
  return value.trim();
}

export function basenameValidationError(value: string) {
  const basename = normalizeBasename(value);
  if (!basename) return "Filename cannot be empty.";
  if (basename === "." || basename === "..") return "Filename cannot be . or ...";
  if (basename.toLowerCase().endsWith(".md")) return "Enter the basename without .md.";
  if ([...basename].some((char) => FORBIDDEN_BASENAME_CHARS.has(char) || char.charCodeAt(0) < 32)) {
    return 'Filename cannot contain / \\ : * ? " < > | or control characters.';
  }
  if (RESERVED_WINDOWS_NAME.test(basename.split(".")[0] ?? basename)) {
    return "Filename cannot be a reserved Windows device name (CON, PRN, AUX, NUL, COM0-9, LPT0-9).";
  }
  return undefined;
}

// The full-width stand-in for each character Obsidian/Windows forbids in a
// filename, so a title keeps its shape (`Fate/stay` → `Fate／stay`) instead of
// being rejected outright. Mirrors the core's `fullwidth_forbidden_char`.
const FULLWIDTH_FORBIDDEN: Record<string, string> = {
  "/": "／",
  "\\": "＼",
  ":": "：",
  "*": "＊",
  "?": "？",
  '"': "＂",
  "<": "＜",
  ">": "＞",
  "|": "｜",
};

// Kept well under the common 255-byte filesystem limit so `.md`, a
// disambiguating suffix, and multi-byte characters all still fit. Matches the
// core's `MAX_DERIVED_BASENAME_BYTES`.
const MAX_DERIVED_BASENAME_BYTES = 200;

const UTF8 = new TextEncoder();

/**
 * Derive a valid basename from an arbitrary title, *replacing* forbidden
 * characters rather than rejecting the whole title as `basenameValidationError`
 * does. Mirrors the core's `derive_basename` (quick-add uses the same rules
 * server-side): NFC-normalize, collapse whitespace, drop control characters,
 * swap forbidden characters for full-width equivalents, strip trailing dots and
 * spaces, sidestep reserved Windows device names, and cap the byte length.
 * Returns `undefined` when nothing usable remains.
 */
export function deriveBasenameFromTitle(title: string): string | undefined {
  let normalized = "";
  let pendingSpace = false;
  for (const character of title.trim().normalize("NFC")) {
    if (/\s/u.test(character)) {
      pendingSpace = normalized.length > 0;
      continue;
    }
    if (/\p{Cc}/u.test(character)) continue;
    if (pendingSpace) {
      normalized += " ";
      pendingSpace = false;
    }
    normalized += FULLWIDTH_FORBIDDEN[character] ?? character;
  }

  let basename = "";
  let bytes = 0;
  for (const character of normalized) {
    bytes += UTF8.encode(character).length;
    if (bytes > MAX_DERIVED_BASENAME_BYTES) break;
    basename += character;
  }
  basename = basename.replace(/[. ]+$/u, "");
  if (!basename) return undefined;

  if (RESERVED_WINDOWS_NAME.test(basename.split(".")[0] ?? basename)) {
    basename += "-";
  }

  return basenameValidationError(basename) ? undefined : basename;
}
