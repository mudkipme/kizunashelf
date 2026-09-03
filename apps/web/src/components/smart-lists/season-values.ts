// The season vocabulary a criteria rule is written in.
//
// A season rule is stored as plain text — `note.season == "Spring 2024"` — so
// that Obsidian, which has no idea what a season is, still reads it as an
// ordinary property test and lands on the same notes. The text follows the
// field's own `seasonLanguage`, the same way the entity editor writes the
// value, so a rule built here matches what this vault's files actually say.
//
// Our evaluator is looser: it compares the *season a value names*, so a rule
// written `Spring 2024` also matches a note whose frontmatter reads `2024年春`.
// That leniency is ours alone — Obsidian only ever matches the literal text,
// which is why a vault mixing season languages agrees with us but not with it.

import { formatSeasonValue, seasonOptions } from "@/components/entities/frontmatter-utils";
import type { SeasonKey, SeasonLanguage } from "@/components/entities/metadata-types";

/// How far back the pickers offer seasons. Nothing reads the vault's real
/// range, so this is simply a window wide enough to cover a catalog: the
/// combobox filters as you type, and both pickers accept a typed-in value for
/// anything outside it.
const yearsBack = 30;
const yearsAhead = 1;

export function normalizeSeasonLanguage(language: string | null | undefined): SeasonLanguage {
  return language === "en" || language === "ja" ? language : "zh";
}

/// The years offered, newest first.
export function seasonYearOptions(today = new Date()): string[] {
  const latest = today.getFullYear() + yearsAhead;
  return Array.from({ length: yearsBack + yearsAhead + 1 }, (_, index) => String(latest - index));
}

/// Every season of every offered year, newest first — the option list for the
/// "is" and "is any of" pickers.
export function seasonValueOptions(language: SeasonLanguage, today = new Date()): string[] {
  return seasonYearOptions(today).flatMap((year) => seasonsOfYear(year, language));
}

/// The years a set of values refer to, in order and deduplicated. Values are
/// already years while the "year is" picker is being edited; they are seasons
/// when the operator was just switched from "is any of", and then the year each
/// season falls in is the sensible carry-over.
export function yearsFrom(values: string[]): string[] {
  const years = values
    .map((value) => /(?:19|20)\d{2}/.exec(value)?.[0])
    .filter((year) => year !== undefined);
  return [...new Set(years)];
}

/// A year's four seasons, in calendar order. "Year is 2024" expands to exactly
/// these: a bare `contains("2024")` would be a *substring* test, which Bases
/// (and we) read as list membership on a multi-season field, so it would miss
/// those notes entirely.
export function seasonsOfYear(year: string, language: SeasonLanguage): string[] {
  return seasonOptions(language).map((season) =>
    formatSeasonValue({ kind: "season", year, season: season.key }, language),
  );
}

/// The years a set of season values covers, but only when the values are
/// *exactly* whole years — every season of each year present, and nothing else.
/// That shape is what "year is" writes, so this recovers it on re-open;
/// anything else is an ordinary "is any of" list.
export function wholeYearsOf(values: string[], language: SeasonLanguage): string[] | undefined {
  const byYear = new Map<string, Set<SeasonKey>>();
  for (const value of values) {
    const parsed = parseSeasonKeys(value, language);
    if (!parsed) return undefined;
    const seasons = byYear.get(parsed.year) ?? new Set<SeasonKey>();
    seasons.add(parsed.season);
    byYear.set(parsed.year, seasons);
  }
  const complete = [...byYear.values()].every(
    (seasons) => seasons.size === seasonOptions(language).length,
  );
  if (!complete || byYear.size === 0) return undefined;
  return [...byYear.keys()];
}

/// Splits a value back into its year and season, but only when it is written
/// the way this field's season language writes them — a hand-written value in
/// another language stays opaque here, so it round-trips as typed rather than
/// being silently rewritten.
function parseSeasonKeys(
  value: string,
  language: SeasonLanguage,
): { year: string; season: SeasonKey } | undefined {
  const year = /(?:19|20)\d{2}/.exec(value)?.[0];
  if (!year) return undefined;
  const season = seasonOptions(language).find(
    (season) => formatSeasonValue({ kind: "season", year, season: season.key }, language) === value,
  );
  return season ? { year, season: season.key } : undefined;
}
