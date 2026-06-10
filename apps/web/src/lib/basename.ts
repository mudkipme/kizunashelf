const FORBIDDEN_BASENAME_CHARS = new Set(['\\', "/", ":", "*", "?", '"', "<", ">", "|"]);

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
  return undefined;
}
