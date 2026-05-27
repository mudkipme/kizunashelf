import { useEffect, useState } from "react";
import { getAnalytics } from "@kizunashelf/api-contract";
import { Link } from "react-router-dom";

import { apiFetch, errorMessage } from "@/api/client";
import { AnalyticsSection } from "@/components/analytics/analytics-section";
import { BarList } from "@/components/analytics/bar-list";
import { CoverageList } from "@/components/analytics/coverage-list";
import { EntityMiniList } from "@/components/analytics/entity-mini-list";
import { StatTile } from "@/components/analytics/stat-tile";
import { AppFrame } from "@/components/layout/app-frame";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { relationFieldHref, relationTargetHref } from "@/lib/relations";
import type { AnalyticsResponse } from "@/types/api";

type StatisticsState = {
  data?: AnalyticsResponse;
  loading: boolean;
  error?: string;
};

export function StatisticsPage() {
  const [state, setState] = useState<StatisticsState>({ loading: true });

  useEffect(() => {
    void loadAnalytics();
  }, []);

  async function loadAnalytics() {
    setState({ loading: true });
    try {
      const data = await getAnalytics(undefined, apiFetch);
      setState({ data, loading: false });
    } catch (error) {
      setState({ loading: false, error: errorMessage(error) });
    }
  }

  const data = state.data;
  const maxTypeCount = Math.max(1, ...(data?.distributions.byType.map((item) => item.count) ?? [1]));
  const maxTimelineCount = Math.max(1, ...(data?.timeline.years.map((item) => item.count) ?? [1]));

  return (
    <AppFrame error={state.error}>
      <div className="mx-auto flex w-full max-w-7xl flex-col gap-4 p-4">
        <header className="flex flex-wrap items-end justify-between gap-3">
          <div className="min-w-0">
            <h1 className="truncate text-base font-semibold">Memory Analytics</h1>
            <p className="mt-1 text-xs text-muted-foreground">
              {state.loading
                ? "Loading"
                : data
                  ? `Updated ${data.generatedAt.slice(0, 10)}`
                  : "No analytics data"}
            </p>
          </div>
        </header>

        {state.loading ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">
            Loading
          </div>
        ) : null}

        {data ? (
          <>
            <section className="grid grid-cols-2 gap-3 lg:grid-cols-5">
              <StatTile label="Entities" value={data.totals.entities} />
              <StatTile label="Relations" value={data.totals.relations} />
              <StatTile label="Dated" value={data.totals.datedEntities} />
              <StatTile label="Connected" value={data.totals.connectedEntities} />
              <StatTile label="Unresolved" value={data.totals.unresolvedRelations} />
            </section>

            <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
              <AnalyticsSection title="Type Distribution">
                <BarList
                  max={maxTypeCount}
                  items={data.distributions.byType.map((item) => ({
                    name: item.label,
                    count: item.count,
                    href: `/library?type=${encodeURIComponent(item.id)}`,
                  }))}
                />
              </AnalyticsSection>

              <AnalyticsSection title="Status Distribution">
                <BarList items={data.distributions.byStatus} />
              </AnalyticsSection>
            </div>

            <AnalyticsSection title="Coverage" subtitle="How complete the browsable memory graph is">
              <CoverageList items={data.coverage} />
            </AnalyticsSection>

            <div className="grid grid-cols-1 gap-4 xl:grid-cols-[minmax(0,1.25fr)_minmax(0,0.75fr)]">
              <AnalyticsSection
                title="Timeline"
                subtitle={`${data.timeline.totalDated.toLocaleString()} entities have parseable year data`}
              >
                <div className="flex flex-col gap-3">
                  {data.timeline.years.slice(0, 16).map((year) => (
                    <div key={year.year} className="rounded-md border p-3">
                      <div className="flex items-center justify-between gap-2 text-xs">
                        <span className="font-medium">{year.year}</span>
                        <span className="tabular-nums text-muted-foreground">
                          {year.count.toLocaleString()}
                        </span>
                      </div>
                      <div className="mt-2 h-2 rounded-sm bg-muted">
                        <div
                          className="h-2 rounded-sm bg-primary"
                          style={{
                            width: `${Math.max(2, Math.round((year.count / maxTimelineCount) * 100))}%`,
                          }}
                        />
                      </div>
                      <div className="mt-2 flex flex-wrap gap-1">
                        {year.byType.slice(0, 6).map((type) => (
                          <Badge key={type.name} variant="outline">
                            {type.name} {type.count}
                          </Badge>
                        ))}
                      </div>
                      <div className="mt-2">
                        <EntityMiniList items={year.examples.slice(0, 4)} />
                      </div>
                    </div>
                  ))}
                </div>
              </AnalyticsSection>

              <div className="flex flex-col gap-4">
                <AnalyticsSection title="Recent Seasons">
                  <BarList items={data.timeline.seasons} />
                </AnalyticsSection>
                <AnalyticsSection title="Recent Months">
                  <BarList items={data.timeline.months} />
                </AnalyticsSection>
              </div>
            </div>

            <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
              <AnalyticsSection title="Relation Fields">
                <BarList
                  items={data.distributions.byRelationField.map((item) => ({
                    ...item,
                    href: relationFieldHref(item.name),
                  }))}
                />
              </AnalyticsSection>

              <AnalyticsSection title="Source -> Target Types">
                <BarList items={data.distributions.bySourceTargetType} />
              </AnalyticsSection>
            </div>

            <AnalyticsSection title="Top Relation Hubs">
              <div className="grid grid-cols-1 gap-2 lg:grid-cols-2">
                {data.relations.topTargets.map((target) => (
                  <Link
                    key={target.key}
                    to={relationTargetHref(target.fields[0]?.name ?? "related", target)}
                    className="rounded-md border p-3 hover:bg-accent"
                  >
                    <div className="flex min-w-0 items-center gap-2">
                      <span className="min-w-0 truncate text-sm font-medium">
                        {target.targetTitle}
                      </span>
                      {target.targetTypeLabel ? (
                        <Badge variant="outline">{target.targetTypeLabel}</Badge>
                      ) : null}
                      <Badge variant="secondary">{target.count}</Badge>
                    </div>
                    <div className="mt-2 flex flex-wrap gap-1">
                      {target.fields.slice(0, 4).map((field) => (
                        <Badge key={field.name} variant="outline">
                          {field.name} {field.count}
                        </Badge>
                      ))}
                    </div>
                  </Link>
                ))}
              </div>
            </AnalyticsSection>

            <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
              <AnalyticsSection title="Missing Cover">
                <EntityMiniList items={data.dataQuality.missingCover} />
              </AnalyticsSection>
              <AnalyticsSection title="Missing External Refs">
                <EntityMiniList items={data.dataQuality.missingExternalRefs} />
              </AnalyticsSection>
              <AnalyticsSection title="Missing Summary">
                <EntityMiniList items={data.dataQuality.missingSummary} />
              </AnalyticsSection>
              <AnalyticsSection title="Isolated Nodes">
                <EntityMiniList items={data.dataQuality.isolated} />
              </AnalyticsSection>
            </div>

            <AnalyticsSection title="Unresolved Relations">
              {data.relations.unresolved.examples.length > 0 ? (
                <div className="flex flex-col gap-1">
                  {data.relations.unresolved.examples.map((relation) => (
                    <div
                      key={`${relation.sourceId}-${relation.field}-${relation.targetTitle}`}
                      className="flex min-w-0 items-center gap-2 rounded-md px-2 py-1 text-xs"
                    >
                      <Badge variant="outline">{relation.field}</Badge>
                      <span className="min-w-0 truncate">{relation.targetTitle}</span>
                      <Button asChild variant="ghost" size="sm" className="ml-auto">
                        <Link to={`/entities/${encodeURIComponent(relation.sourceId)}`}>
                          Source
                        </Link>
                      </Button>
                    </div>
                  ))}
                </div>
              ) : (
                <div className="text-sm text-muted-foreground">No unresolved relation targets</div>
              )}
            </AnalyticsSection>
          </>
        ) : null}
      </div>
    </AppFrame>
  );
}
