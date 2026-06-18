import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";

import { errorMessage } from "@/api/client";
import { analyticsQuery, configQuery } from "@/api/queries";
import { AnalyticsSection } from "@/components/analytics/analytics-section";
import { BarList } from "@/components/analytics/bar-list";
import { CoverageList } from "@/components/analytics/coverage-list";
import { EntityMiniList } from "@/components/analytics/entity-mini-list";
import { StatTile } from "@/components/analytics/stat-tile";
import { AppFrame } from "@/components/layout/app-frame";
import { Badge } from "@/components/ui/badge";
import { fieldLabelAcrossTypes, fieldLabelsByType } from "@/lib/type-config";

export function StatisticsPage() {
  const analytics = useQuery(analyticsQuery());
  const config = useQuery(configQuery());
  // Config only supplies type/field labels here; don't block rendering or fail
  // the whole page on it. Analytics is the load-bearing query.
  const loading = analytics.isPending;
  const error = analytics.error;
  const data = analytics.data;
  const labelsByType = useMemo(() => fieldLabelsByType(config.data?.types), [config.data]);
  const maxTypeCount = Math.max(1, ...(data?.distributions.byType.map((item) => item.count) ?? [1]));
  const maxTimelineCount = Math.max(1, ...(data?.timeline.years.map((item) => item.count) ?? [1]));

  return (
    <AppFrame error={error ? errorMessage(error) : undefined}>
      <div className="mx-auto flex w-full max-w-7xl flex-col gap-4 p-4">
        <header className="flex flex-wrap items-end justify-between gap-3">
          <div className="min-w-0">
            <h1 className="truncate text-base font-semibold">Memory Analytics</h1>
            <p className="mt-1 text-xs text-muted-foreground">
              {loading
                ? "Loading"
                : data
                  ? `Updated ${data.generatedAt.slice(0, 10)}`
                  : "No analytics data"}
            </p>
          </div>
        </header>

        {loading ? (
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

              <AnalyticsSection title="Relation Fields">
                <BarList
                  items={data.distributions.byRelationField.map((item) => ({
                    ...item,
                    name: fieldLabelAcrossTypes(config.data?.types, item.name),
                  }))}
                />
              </AnalyticsSection>
            </div>

            <AnalyticsSection title="Coverage" description="How complete the browsable memory graph is">
              <CoverageList items={data.coverage} />
            </AnalyticsSection>

            <div className="grid grid-cols-1 gap-4 xl:grid-cols-[minmax(0,1.25fr)_minmax(0,0.75fr)]">
              <AnalyticsSection
                title="Timeline"
                description={`${data.timeline.totalDated.toLocaleString()} entities have parseable year data`}
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
                        <EntityMiniList items={year.examples.slice(0, 4)} labelsByType={labelsByType} />
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

          </>
        ) : null}
      </div>
    </AppFrame>
  );
}
