const FORBIDDEN_BASENAME_CHARS = /[\\/:*?"<>|\x00-\x1f]/;

export function normalizeBasename(value: string) {
  return value.trim();
}

export function basenameValidationError(value: string) {
  const basename = normalizeBasename(value);
  if (!basename) return "Filename cannot be empty.";
  if (basename === "." || basename === "..") return "Filename cannot be . or ...";
  if (basename.toLowerCase().endsWith(".md")) return "Enter the basename without .md.";
  if (FORBIDDEN_BASENAME_CHARS.test(basename)) {
    return 'Filename cannot contain / \\ : * ? " < > | or control characters.';
  }
  return undefined;
}
