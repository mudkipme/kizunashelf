//! Step two: the planned changes, reviewed before anything is written.
//!
//! A plan can run to thousands of rows, so the list renders a bounded window
//! (`PLAN_WINDOW`) and each row is memoized — the wizard's per-item toggles
//! would otherwise re-render every row on every click.

import { plural } from "@lingui/core/macro";
import { Plural, Trans, useLingui } from "@lingui/react/macro";
import { memo, useEffect, useMemo, useRef, useState } from "react";
import { Link } from "react-router-dom";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";
import type { ImportJob, ImportPlanBucket, ImportPlanItem } from "@/types/api";

import { bucketLabel, reviewReasonLabel, statusLabel } from "./import-labels";

export function PlanReview({
  plan,
  onBucketType,
  effectiveType,
  skip,
  onToggleSkip,
  typeLabels,
  providerLabels,
}: {
  plan: ImportJob["plan"];
  onBucketType: (bucket: string, type: string) => void;
  effectiveType: (bucket: ImportPlanBucket) => string | undefined;
  skip: ReadonlySet<number>;
  onToggleSkip: (index: number) => void;
  typeLabels: Map<string, string>;
  providerLabels: Map<string, string>;
}) {
  const { i18n } = useLingui();
  const items = useMemo(() => plan?.items ?? [], [plan]);
  const counts = useMemo(
    () => ({
      willCreate: items.filter((item) => item.state === "willCreate").length,
      exists: items.filter((item) => item.state === "exists").length,
      needsReview: items.filter((item) => item.state === "needsReview").length,
    }),
    [items],
  );

  // Large exports (a MAL/Goodreads library is routinely 1,000–5,000 rows) are
  // revealed in windows instead of mounting every row at once: a sentinel below
  // the list grows the window as the user scrolls, and "Show all" mounts the
  // rest for anyone who wants to Ctrl-F the plan. Skip decisions key off
  // `item.index`, so windowing never affects what the commit sends.
  const [visibleCount, setVisibleCount] = useState(PLAN_WINDOW);
  const sentinelRef = useRef<HTMLDivElement | null>(null);
  const total = items.length;
  const hasMore = visibleCount < total;
  useEffect(() => {
    const sentinel = sentinelRef.current;
    if (!sentinel || !hasMore) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          setVisibleCount((prev) => Math.min(prev + PLAN_WINDOW, total));
        }
      },
      // Grow before the sentinel is actually on screen, so scrolling feels
      // continuous rather than stopping at each window edge.
      { rootMargin: "600px" },
    );
    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [hasMore, total]);

  if (!plan) return null;
  const ambiguous = plan.buckets.filter((bucket) => bucket.candidateTypes.length > 1);
  const unmatched = plan.buckets.filter((bucket) => bucket.candidateTypes.length === 0);
  const unmatchedLabels = unmatched
    .map(
      (bucket) =>
        `${providerLabels.get(bucket.provider) ?? bucket.provider} ${bucketLabel(i18n, bucket.provider, bucket.bucket)}`,
    )
    .join(", ");

  return (
    <section className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
        <Badge variant="secondary">
          <Plural value={items.length} one="# found" other="# found" />
        </Badge>
        <span className="tabular-nums">
          <Plural value={counts.willCreate} one="# to create" other="# to create" />
        </span>
        {counts.exists > 0 ? (
          <span>
            ·{" "}
            <Plural value={counts.exists} one="# already in library" other="# already in library" />
          </span>
        ) : null}
        {counts.needsReview > 0 ? (
          <span className="text-amber-700 dark:text-amber-400">
            · <Plural value={counts.needsReview} one="# needs review" other="# need review" />
          </span>
        ) : null}
      </div>

      {ambiguous.length > 0 ? (
        <div className="flex flex-col gap-2 rounded-md border p-3">
          <span className="text-sm font-medium">
            <Trans>Choose a type</Trans>
          </span>
          {ambiguous.map((bucket) => (
            <label
              key={`${bucket.provider}:${bucket.bucket}`}
              className="flex flex-wrap items-center justify-between gap-2 text-sm"
            >
              <span className="text-muted-foreground">
                {providerLabels.get(bucket.provider) ?? bucket.provider} ·{" "}
                {bucketLabel(i18n, bucket.provider, bucket.bucket)}
              </span>
              <Select
                value={effectiveType(bucket) ?? ""}
                onChange={(event) => onBucketType(bucket.bucket, event.target.value)}
                className="w-48"
              >
                {bucket.candidateTypes.map((type) => (
                  <option key={type} value={type}>
                    {typeLabels.get(type) ?? type}
                  </option>
                ))}
              </Select>
            </label>
          ))}
        </div>
      ) : null}

      {unmatched.length > 0 ? (
        <div className="rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-xs text-amber-700 dark:text-amber-400">
          <Trans>
            No entity type maps {unmatchedLabels}. Add an external-reference field for it in
            Settings to import these.
          </Trans>
        </div>
      ) : null}

      <ul className="flex flex-col gap-1.5">
        {items.slice(0, visibleCount).map((item) => (
          <PlanItemRow
            key={item.index}
            item={item}
            skipped={skip.has(item.index)}
            onToggleSkip={onToggleSkip}
            providerLabels={providerLabels}
          />
        ))}
      </ul>
      {hasMore ? (
        <div
          ref={sentinelRef}
          className="flex items-center justify-center gap-3 p-2 text-xs text-muted-foreground"
        >
          <span className="tabular-nums">
            <Trans>
              Showing {visibleCount} of {total}
            </Trans>
          </span>
          <Button variant="outline" size="sm" onClick={() => setVisibleCount(total)}>
            <Trans>Show all</Trans>
          </Button>
        </div>
      ) : null}
    </section>
  );
}

/// How many plan rows mount per window; the scroll sentinel adds another
/// window each time it comes near the viewport.
const PLAN_WINDOW = 200;

// Memoized (with a stable `onToggleSkip`) so ticking one checkbox re-renders
// one row, not every mounted row of a multi-thousand-item plan. The
// `content-visibility` classes let the browser skip layout/paint for rows
// scrolled out of view once mounted.
const PlanItemRow = memo(function PlanItemRow({
  item,
  skipped,
  onToggleSkip,
  providerLabels,
}: {
  item: ImportPlanItem;
  skipped: boolean;
  onToggleSkip: (index: number) => void;
  providerLabels: Map<string, string>;
}) {
  const { t, i18n } = useLingui();
  const creatable = item.state === "willCreate";
  return (
    <li
      className={[
        "flex items-start gap-3 rounded-md border p-2.5 text-sm",
        "[contain-intrinsic-size:auto_4rem] [content-visibility:auto]",
        skipped ? "opacity-50" : "",
      ].join(" ")}
    >
      {creatable ? (
        <input
          type="checkbox"
          checked={!skipped}
          onChange={() => onToggleSkip(item.index)}
          className="mt-1"
          aria-label={t`Include ${item.title}`}
        />
      ) : (
        <span className="mt-1 w-4" />
      )}
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <span className="truncate font-medium">{item.title}</span>
          <StatePill item={item} />
        </div>
        <div className="mt-1 flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
          <span className="rounded border px-1.5 py-0.5">
            {providerLabels.get(item.provider) ?? item.provider ?? t`no provider`}
          </span>
          <span className="rounded border px-1.5 py-0.5">
            {bucketLabel(i18n, item.provider, item.bucket)}
          </span>
          <UserDataSummary item={item} />
        </div>
      </div>
    </li>
  );
});

function StatePill({ item }: { item: ImportPlanItem }) {
  const { i18n } = useLingui();
  if (item.state === "exists") {
    return item.existing ? (
      <Link
        to={`/entities/${encodeURIComponent(item.existing.id)}`}
        className="rounded-full bg-emerald-500/15 px-2 py-0.5 text-xs font-medium text-emerald-700 hover:underline dark:text-emerald-400"
      >
        <Trans>In library ✓</Trans>
      </Link>
    ) : (
      <span className="rounded-full bg-emerald-500/15 px-2 py-0.5 text-xs font-medium text-emerald-700 dark:text-emerald-400">
        <Trans>In library ✓</Trans>
      </span>
    );
  }
  if (item.state === "needsReview") {
    return (
      <span className="rounded-full bg-amber-500/15 px-2 py-0.5 text-xs font-medium text-amber-700 dark:text-amber-400">
        {i18n._(reviewReasonLabel(item.reviewReason))}
      </span>
    );
  }
  return null;
}

function UserDataSummary({ item }: { item: ImportPlanItem }) {
  const { t, i18n } = useLingui();
  const parts: string[] = [];
  const data = item.userData;
  if (data.status) parts.push(statusLabel(i18n, data.status));
  if (typeof data.score10 === "number") parts.push(`★ ${data.score10}`);
  if (typeof data.watchedCount === "number") {
    parts.push(plural(data.watchedCount, { one: "# watched", other: "# watched" }));
  }
  if (data.completed) parts.push(data.completed);
  if (data.hasNotes) parts.push(t`notes`);
  if (parts.length === 0) return null;
  return <span>{parts.join(" · ")}</span>;
}
