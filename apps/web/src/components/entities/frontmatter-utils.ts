import type {
  FrontmatterDraft,
  FrontmatterObject,
  FrontmatterValue,
  SeasonKey,
  SeasonLanguage,
  SeasonRow,
} from "./metadata-types";

export function normalizeFrontmatter(value: Record<string, unknown>) {
  return Object.fromEntries(
    Object.entries(value).map(([key, item]) => [key, normalizeFrontmatterValue(item)]),
  ) as FrontmatterDraft;
}

function normalizeFrontmatterValue(value: unknown): FrontmatterValue {
  if (value === null || ["boolean", "number", "string"].includes(typeof value)) {
    return value as FrontmatterValue;
  }
  if (Array.isArray(value)) {
    return value.map(normalizeFrontmatterValue);
  }
  if (typeof value === "object" && value) {
    return Object.fromEntries(
      Object.entries(value).map(([key, item]) => [key, normalizeFrontmatterValue(item)]),
    );
  }
  return String(value ?? "");
}

export function valueToText(value: FrontmatterValue | undefined): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return JSON.stringify(value);
}

export function numberOrString(value: string) {
  if (!value) return null;
  const number = Number(value);
  return Number.isFinite(number) && String(number) === value ? number : value;
}

export function isFrontmatterObject(
  value: FrontmatterValue | undefined,
): value is FrontmatterObject {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function isScalarFrontmatterValue(
  value: FrontmatterValue | undefined,
): value is null | boolean | number | string {
  return value === null || ["boolean", "number", "string"].includes(typeof value);
}

export function parseScalarValue(value: string): FrontmatterValue {
  const trimmed = value.trim();
  if (!trimmed) return null;
  if (trimmed === "true") return true;
  if (trimmed === "false") return false;
  const number = Number(trimmed);
  if (Number.isFinite(number) && String(number) === trimmed) return number;
  return value;
}

export function withCurrentOption(options: string[], current: string) {
  if (!current || options.includes(current)) return options;
  return [current, ...options];
}

export function currentOptions(value: FrontmatterValue | undefined) {
  const options = new Set<string>();
  const currentValues = Array.isArray(value) ? value.map(valueToText) : [valueToText(value)];
  for (const current of currentValues) {
    if (current) options.add(current);
  }
  return [...options];
}

export function uniqueStrings(values: string[]) {
  return values.filter((value, index) => value && values.indexOf(value) === index);
}

export function listDisplayValues(value: FrontmatterValue | undefined, wikilinks: boolean) {
  const values = Array.isArray(value)
    ? value
    : value === null || value === undefined || value === ""
      ? []
      : [value];
  return values
    .map(valueToText)
    .map((item) => (wikilinks ? stripWikilink(item) : item))
    .map((item) => normalizeListItem(item, wikilinks))
    .filter(Boolean);
}

export function parseSeasonValue(value: string): SeasonRow {
  const year = /(?:19|20)\d{2}/.exec(value)?.[0];
  const normalized = value.toLowerCase();
  let season: SeasonKey | undefined;
  // Match the bare CJK kanji (冬/夏/秋/春) so both the Chinese 季-suffixed form
  // (冬季) and the Japanese short form (冬) parse — `formatSeasonValue` emits the
  // latter for `ja`, so it must round-trip.
  if (value.includes("冬") || normalized.includes("winter")) season = "winter";
  else if (value.includes("夏") || normalized.includes("summer")) season = "summer";
  else if (value.includes("秋") || normalized.includes("autumn") || normalized.includes("fall"))
    season = "autumn";
  else if (value.includes("春") || normalized.includes("spring")) season = "spring";
  if (!year || !season) return { kind: "raw", value };
  return { kind: "season", year, season };
}

export function formatSeasonValue(
  row: Extract<SeasonRow, { kind: "season" }>,
  language: SeasonLanguage,
) {
  const year = row.year.trim();
  if (language === "en") {
    const label =
      seasonOptions(language).find((season) => season.key === row.season)?.label ?? "Spring";
    return `${label} ${year}`;
  }
  const label =
    seasonOptions(language).find((season) => season.key === row.season)?.label ?? "春季";
  return `${year}年${label}`;
}

export function seasonOptions(language: SeasonLanguage): Array<{ key: SeasonKey; label: string }> {
  if (language === "en") {
    return [
      { key: "winter", label: "Winter" },
      { key: "spring", label: "Spring" },
      { key: "summer", label: "Summer" },
      { key: "autumn", label: "Autumn" },
    ];
  }
  if (language === "ja") {
    // Japanese seasons drop the 季 suffix that Chinese uses (春 vs 春季).
    return [
      { key: "winter", label: "冬" },
      { key: "spring", label: "春" },
      { key: "summer", label: "夏" },
      { key: "autumn", label: "秋" },
    ];
  }
  return [
    { key: "winter", label: "冬季" },
    { key: "spring", label: "春季" },
    { key: "summer", label: "夏季" },
    { key: "autumn", label: "秋季" },
  ];
}

export function parseDateValue(value: string) {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value.trim());
  if (!match) return undefined;
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const date = new Date(year, month - 1, day);
  if (date.getFullYear() !== year || date.getMonth() !== month - 1 || date.getDate() !== day) {
    return undefined;
  }
  return date;
}

export function formatDateValue(date: Date) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

export function normalizeListItem(value: string, wikilinks: boolean) {
  const trimmed = value.trim();
  if (!trimmed) return "";
  return wikilinks ? stripWikilink(trimmed) : trimmed;
}

export function toWikilink(value: string) {
  const target = stripWikilink(value).trim();
  return target ? `[[${target}]]` : "";
}

export function stripWikilink(value: string) {
  const trimmed = value.trim();
  const match = /^\[\[(.*?)(?:\|.*?)?\]\]$/.exec(trimmed);
  return match?.[1]?.trim() ?? trimmed;
}

export function isAbortError(error: unknown) {
  return error instanceof DOMException && error.name === "AbortError";
}
