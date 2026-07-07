import { entityTitle, primaryLanguage } from "@/lib/title-language";
import type { EntitySummary } from "@/types/api";

export const allEntityFilter = "all";

type EntityFilterOption = {
  value: string;
  label: string;
  count: number;
};

export function entityTypeOptions(entities: EntitySummary[]): EntityFilterOption[] {
  const counts = countBy(entities, (entity) => entity.type);
  const labels = new Map(entities.map((entity) => [entity.type, entity.typeLabel]));
  return [...counts.entries()]
    .map(([value, count]) => ({ value, label: labels.get(value) ?? value, count }))
    .sort((a, b) => a.label.localeCompare(b.label));
}

export function entityDateOptions(entities: EntitySummary[]): EntityFilterOption[] {
  const counts = new Map<string, number>();
  for (const entity of entities) {
    const years = new Set(entity.dates.map((date) => entityDateYear(date.value)).filter(Boolean));
    for (const year of years) counts.set(year, (counts.get(year) ?? 0) + 1);
  }
  return [...counts.entries()]
    .map(([year, count]) => ({ value: `year:${year}`, label: year, count }))
    .sort((a, b) => b.label.localeCompare(a.label));
}

export function entityMatchesQuery(
  entity: EntitySummary,
  query: string,
  extraValues: string[] = [],
) {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return true;
  const values = [
    entity.title,
    entity.summary ?? "",
    entity.basename,
    entity.path,
    ...Object.values(entity.titles),
    ...extraValues,
  ];
  return values.some((value) => value.toLowerCase().includes(normalized));
}

export function entityMatchesDate(entity: EntitySummary, selectedDate: string) {
  if (selectedDate === allEntityFilter) return true;
  if (selectedDate === "dated") return entity.dates.length > 0;
  if (selectedDate === "undated") return entity.dates.length === 0;
  if (selectedDate.startsWith("year:")) {
    const year = selectedDate.slice("year:".length);
    return entity.dates.some((date) => entityDateYear(date.value) === year);
  }
  return true;
}

export function compareEntitiesByTypeThenTitle(
  a: EntitySummary,
  b: EntitySummary,
  language?: string,
) {
  // `language` may be the full preference (e.g. `zh-Hant`): titles are looked
  // up by its bare primary subtag, while collation uses the full tag so
  // Traditional Chinese sorts as Traditional.
  const locale = language?.trim() ? language : undefined;
  if (a.typeLabel !== b.typeLabel) return a.typeLabel.localeCompare(b.typeLabel, locale);
  const titleLanguage = locale ? primaryLanguage(locale) : undefined;
  const titleA = titleLanguage ? entityTitle(a, titleLanguage) : a.title;
  const titleB = titleLanguage ? entityTitle(b, titleLanguage) : b.title;
  return titleA.localeCompare(titleB, locale);
}

function countBy<T>(items: T[], key: (item: T) => string) {
  const counts = new Map<string, number>();
  for (const item of items) {
    const value = key(item);
    counts.set(value, (counts.get(value) ?? 0) + 1);
  }
  return counts;
}

function entityDateYear(value: string) {
  const match = /^(\d{4})/.exec(value);
  return match?.[1] ?? "";
}
