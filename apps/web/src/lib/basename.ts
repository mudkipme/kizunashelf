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
