export type ParsedEntityDate = {
  year: number;
  month?: number;
  day?: number;
  season?: string;
};

const seasonOrder = new Map([
  ["冬季", 0],
  ["春季", 1],
  ["夏季", 2],
  ["秋季", 3],
]);

const seasonEndDate = new Map([
  ["冬季", { month: 3, day: 31 }],
  ["春季", { month: 6, day: 30 }],
  ["夏季", { month: 9, day: 30 }],
  ["秋季", { month: 12, day: 31 }],
]);

const exactDatePattern =
  /\b((?:19|20)\d{2})[-/.](\d{1,2})[-/.](\d{1,2})\b|((?:19|20)\d{2})年(\d{1,2})月(\d{1,2})日/;

export function parseEntityDate(value: string | undefined): ParsedEntityDate | undefined {
  if (!value) return undefined;
  const exact = exactDateParts(value);
  if (exact) {
    return {
      year: exact.year,
      month: exact.month,
      day: exact.day,
      season: seasonForMonth(exact.month),
    };
  }

  const year = value.match(/\b(19|20)\d{2}\b/)?.[0] ?? value.match(/(19|20)\d{2}年/)?.[0];
  if (!year) return undefined;
  const parsedYear = Number(year.slice(0, 4));
  const month = value.match(/\b(19|20)\d{2}[-/.](\d{1,2})/)?.[2];
  const parsedMonth = month ? clampNumber(Number(month), 1, 12) : undefined;
  const season = value.match(/年(春季|夏季|秋季|冬季)/)?.[1];

  return {
    year: parsedYear,
    month: parsedMonth,
    season: season ?? (parsedMonth ? seasonForMonth(parsedMonth) : undefined),
  };
}

export function dateSortKey(value: string | undefined) {
  if (!value) return undefined;

  const exact = parseExactDate(value);
  if (exact) return exact;

  const parsed = parseEntityDate(value);
  if (!parsed?.season) return value;

  const end = seasonEndDate.get(parsed.season);
  if (!end) return value;
  return normalizeDate(parsed.year, end.month, end.day) ?? value;
}

export function seasonCompareValue(season: string) {
  return seasonOrder.get(season) ?? Number.MIN_SAFE_INTEGER;
}

export function parseExactDate(value: string | undefined): string | undefined {
  const parts = exactDateParts(value);
  return parts ? normalizeDate(parts.year, parts.month, parts.day) : undefined;
}

export function exactDateParts(value: string | undefined) {
  if (!value) return undefined;
  const match = exactDatePattern.exec(value);
  if (!match) return undefined;

  return {
    year: Number(match[1] ?? match[4]),
    month: Number(match[2] ?? match[5]),
    day: Number(match[3] ?? match[6]),
  };
}

export function normalizeDate(year: number, month: number, day: number) {
  if (!Number.isInteger(year) || !Number.isInteger(month) || !Number.isInteger(day)) {
    return undefined;
  }
  const date = new Date(Date.UTC(year, month - 1, day));
  if (
    date.getUTCFullYear() !== year ||
    date.getUTCMonth() !== month - 1 ||
    date.getUTCDate() !== day
  ) {
    return undefined;
  }

  return `${year}-${String(month).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
}

export function isInMonth(date: string, year: number, month: number) {
  return date.startsWith(`${year}-${String(month).padStart(2, "0")}-`);
}

function seasonForMonth(month: number) {
  if (month <= 3) return "冬季";
  if (month <= 6) return "春季";
  if (month <= 9) return "夏季";
  return "秋季";
}

function clampNumber(value: number, min: number, max: number) {
  if (!Number.isFinite(value)) return min;
  return Math.floor(Math.min(max, Math.max(min, value)));
}
