import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";

import { errorMessage } from "@/api/client";
import { analyticsQuery, configQuery } from "@/api/queries";
import { ActivityHeatmap } from "@/components/analytics/activity-heatmap";
import { AnalyticsSection } from "@/components/analytics/analytics-section";
import { BarList } from "@/components/analytics/bar-list";
import { StatTile } from "@/components/analytics/stat-tile";
import { AppFrame } from "@/components/layout/app-frame";
import { Placeholder } from "@/components/ui/placeholder";
import { fieldLabelAcrossTypes } from "@/lib/type-config";

export function StatisticsPage() {
  const { t } = useLingui();
  const analytics = useQuery(analyticsQuery());
  const config = useQuery(configQuery());
  // Config only supplies type/field labels here; don't block rendering or fail
  // the whole page on it. Analytics is the load-bearing query.
  const loading = analytics.isPending;
  const error = analytics.error;
  const data = analytics.data;
  const maxTypeCount = Math.max(1, ...(data?.distributions.byType.map((item) => item.count) ?? [1]));

  return (
    <AppFrame error={error ? errorMessage(error) : undefined}>
      <div className="mx-auto flex w-full max-w-7xl flex-col gap-4 p-4">
        <header className="flex flex-wrap items-end justify-between gap-3">
          <div className="min-w-0">
            <h1 className="truncate text-base font-semibold"><Trans>Memory Analytics</Trans></h1>
            <p className="mt-1 text-xs text-muted-foreground">
              {loading
                ? t`Loading`
                : data
                  ? t`Updated ${data.generatedAt.slice(0, 10)}`
                  : t`No analytics data`}
            </p>
          </div>
        </header>

        {loading ? (
          <Placeholder>
            <Trans>Loading</Trans>
          </Placeholder>
        ) : null}

        {data ? (
          <>
            <section className="grid grid-cols-2 gap-3 lg:grid-cols-5">
              <StatTile label={t`Entities`} value={data.totals.entities} />
              <StatTile label={t`Connections`} value={data.totals.relations} />
              <StatTile label={t`Dated`} value={data.totals.datedEntities} />
              <StatTile label={t`Connected`} value={data.totals.connectedEntities} />
              <StatTile label={t`Unresolved`} value={data.totals.unresolvedRelations} />
            </section>

            <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
              <AnalyticsSection title={t`Type Distribution`}>
                <BarList
                  max={maxTypeCount}
                  items={data.distributions.byType.map((item) => ({
                    name: item.label,
                    count: item.count,
                    href: `/library?type=${encodeURIComponent(item.id)}`,
                  }))}
                />
              </AnalyticsSection>

              <AnalyticsSection title={t`Connection Fields`}>
                <BarList
                  items={data.distributions.byRelationField.map((item) => ({
                    ...item,
                    name: fieldLabelAcrossTypes(config.data?.types, item.name),
                  }))}
                />
              </AnalyticsSection>
            </div>

            <AnalyticsSection
              title={t`Activity`}
              description={t`Dated entities by month and year — compare this year against past years, filter by type`}
            >
              <ActivityHeatmap activity={data.activity} />
            </AnalyticsSection>
          </>
        ) : null}
      </div>
    </AppFrame>
  );
}
