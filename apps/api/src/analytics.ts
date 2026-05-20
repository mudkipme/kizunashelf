import type { EntitySummary, Library } from "@kizunashelf/core";

import { dateSortKey, parseEntityDate, seasonCompareValue, type ParsedEntityDate } from "./dates";
import {
  buildRelationFieldSummary,
  buildRelationHubs,
  outgoingRelations,
  relationFields,
  relationTypePairs,
} from "./relations";
import { compareString, countBy, getStatusTrackedTypeIds } from "./utils";

export function buildAnalytics(library: Library) {
  const summaries = library.summaries;
  const statusTrackedTypeIds = getStatusTrackedTypeIds(library);
  const statusSummaries = summaries.filter((entity) => statusTrackedTypeIds.has(entity.type));
  const outgoing = outgoingRelations(library);
  const unresolved = outgoing.filter((relation) => !relation.targetId);
  const dated = summaries.flatMap((entity) =>
    entity.dates
      .map((item) => parseEntityDate(item.value))
      .filter((date): date is ParsedEntityDate => Boolean(date))
      .map((date) => ({ entity, date })),
  );
  const datedEntityIds = new Set(dated.map((item) => item.entity.id));
  const withCover = summaries.filter((entity) => Boolean(entity.image));
  const withRefs = summaries.filter((entity) => Object.keys(entity.externalRefs).length > 0);
  const withSummary = summaries.filter((entity) => Boolean(entity.summary));
  const connected = summaries.filter((entity) => entity.relationCount > 0);

  return {
    generatedAt: library.generatedAt,
    totals: {
      entities: summaries.length,
      relations: outgoing.length,
      unresolvedRelations: unresolved.length,
      datedEntities: datedEntityIds.size,
      connectedEntities: connected.length,
    },
    distributions: {
      byType: library.config.types.map((type) => ({
        id: type.id,
        label: type.label,
        count: summaries.filter((entity) => entity.type === type.id).length,
      })),
      byStatus: countBy(statusSummaries, (entity) => entity.status ?? "Unknown"),
      byRelationField: countBy(outgoing, (relation) => relation.field).slice(0, 16),
      bySourceTargetType: relationTypePairs(library).slice(0, 16),
    },
    coverage: [
      buildCoverageMetric("Cover", withCover.length, summaries),
      buildCoverageMetric("External refs", withRefs.length, summaries),
      buildCoverageMetric("Summary", withSummary.length, summaries),
      buildCoverageMetric("Relations", connected.length, summaries),
      buildCoverageMetric(
        "Resolved relation targets",
        outgoing.length - unresolved.length,
        outgoing,
      ),
    ],
    timeline: buildTimeline(dated),
    relations: {
      topFields: relationFields(library)
        .map((field) => buildRelationFieldSummary(library, field))
        .filter((field) => field.edgeCount > 0)
        .slice(0, 12),
      topTargets: buildRelationHubs(library).slice(0, 12),
      unresolved: {
        count: unresolved.length,
        examples: unresolved.slice(0, 12),
      },
    },
    dataQuality: {
      missingCover: summaries.filter((entity) => !entity.image).slice(0, 12),
      missingExternalRefs: summaries
        .filter((entity) => Object.keys(entity.externalRefs).length === 0)
        .slice(0, 12),
      missingSummary: summaries.filter((entity) => !entity.summary).slice(0, 12),
      isolated: summaries.filter((entity) => entity.relationCount === 0).slice(0, 12),
    },
  };
}

function buildCoverageMetric<T>(name: string, count: number, items: T[]) {
  const total = items.length;
  return {
    name,
    count,
    missing: total - count,
    total,
    percent: total === 0 ? 0 : Math.round((count / total) * 100),
  };
}

function buildTimeline(dated: Array<{ entity: EntitySummary; date: ParsedEntityDate }>) {
  const byYear = new Map<number, EntitySummary[]>();
  const bySeason = new Map<string, { year: number; season: string; entities: EntitySummary[] }>();
  const byMonth = new Map<string, EntitySummary[]>();

  for (const item of dated) {
    const yearItems = byYear.get(item.date.year) ?? [];
    yearItems.push(item.entity);
    byYear.set(item.date.year, yearItems);

    if (item.date.season) {
      const key = `${item.date.year} ${item.date.season}`;
      const seasonItems =
        bySeason.get(key) ?? {
          year: item.date.year,
          season: item.date.season,
          entities: [],
        };
      seasonItems.entities.push(item.entity);
      bySeason.set(key, seasonItems);
    }

    if (item.date.month) {
      const key = `${item.date.year}-${String(item.date.month).padStart(2, "0")}`;
      const monthItems = byMonth.get(key) ?? [];
      monthItems.push(item.entity);
      byMonth.set(key, monthItems);
    }
  }

  const years = [...byYear.entries()]
    .sort((a, b) => b[0] - a[0])
    .map(([year, entities]) => ({
      year,
      count: entities.length,
      byType: countBy(entities, (entity) => entity.typeLabel),
      examples: [...entities]
        .sort((a, b) => compareString(dateSortKey(b.dates[0]?.value), dateSortKey(a.dates[0]?.value)))
        .slice(0, 6),
    }));

  return {
    totalDated: dated.length,
    years,
    seasons: [...bySeason.values()]
      .map((item) => ({
        name: `${item.year} ${item.season}`,
        count: item.entities.length,
        year: item.year,
        season: item.season,
      }))
      .sort((a, b) => {
        if (a.year !== b.year) return b.year - a.year;
        return seasonCompareValue(b.season) - seasonCompareValue(a.season);
      })
      .map(({ name, count }) => ({ name, count }))
      .slice(0, 12),
    months: [...byMonth.entries()]
      .map(([name, entities]) => ({ name, count: entities.length }))
      .sort((a, b) => compareString(b.name, a.name))
      .slice(0, 18),
  };
}
