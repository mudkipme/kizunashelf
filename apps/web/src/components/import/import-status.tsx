//! The wizard's write-scope toggles and the running job's progress readout.

import { Plural, Trans } from "@lingui/react/macro";

import { Badge } from "@/components/ui/badge";
import type { ImportJob } from "@/types/api";

export function OptionToggles({
  options,
  onChange,
}: {
  options: { importUserData: boolean; importEpisodes: boolean; markProgress: boolean };
  onChange: (next: { importUserData: boolean; importEpisodes: boolean; markProgress: boolean }) => void;
}) {
  const toggle = (key: keyof typeof options) => onChange({ ...options, [key]: !options[key] });
  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-sm">
      <label className="flex items-center gap-1.5">
        <input type="checkbox" checked={options.importUserData} onChange={() => toggle("importUserData")} />
        <Trans>Status, score & dates</Trans>
      </label>
      <label className="flex items-center gap-1.5">
        <input type="checkbox" checked={options.importEpisodes} onChange={() => toggle("importEpisodes")} />
        <Trans>Episodes</Trans>
      </label>
      <label className="flex items-center gap-1.5">
        <input
          type="checkbox"
          checked={options.markProgress}
          disabled={!options.importEpisodes}
          onChange={() => toggle("markProgress")}
        />
        <Trans>Mark watched progress</Trans>
      </label>
    </div>
  );
}

export function JobProgress({ job }: { job: ImportJob }) {
  const percent = job.total > 0 ? Math.round((job.processed / job.total) * 100) : 100;
  const errors = job.errors ?? [];
  const done = job.status === "completed" || job.status === "cancelled";
  return (
    <section className="rounded-md border p-3">
      <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
        <span className="capitalize text-foreground">
          {done ? <Trans>Import complete</Trans> : <Trans>Importing…</Trans>}
        </span>
        <span className="tabular-nums">
          <Trans>
            {job.processed} / {job.total} processed
          </Trans>
        </span>
        <Badge variant="outline">
          <Plural value={job.created} one="# created" other="# created" />
        </Badge>
        {job.skipped > 0 ? (
          <span>
            <Plural value={job.skipped} one="# skipped" other="# skipped" />
          </span>
        ) : null}
        {job.failed > 0 ? (
          <span className="text-destructive">
            <Plural value={job.failed} one="# failed" other="# failed" />
          </span>
        ) : null}
      </div>
      <div className="mt-2 h-2 rounded-sm bg-muted">
        <div className="h-2 rounded-sm bg-primary" style={{ width: `${percent}%` }} />
      </div>
      {job.episodesTotal != null && !done ? (
        // The post-create enrichment phase (one provider fetch per created
        // entity) can outlast the create phase, so it gets its own bar — without
        // it the job looks stuck on a full bar while still committing.
        <>
          <div className="mt-2 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
            <span>
              <Trans>Fetching episodes…</Trans>
            </span>
            <span className="tabular-nums">
              {job.episodesProcessed ?? 0} / {job.episodesTotal}
            </span>
          </div>
          <div className="mt-2 h-2 rounded-sm bg-muted">
            <div
              className="h-2 rounded-sm bg-primary"
              style={{
                width: `${Math.round(((job.episodesProcessed ?? 0) / Math.max(job.episodesTotal, 1)) * 100)}%`,
              }}
            />
          </div>
        </>
      ) : null}
      {errors.length > 0 ? (
        <ul className="mt-2 space-y-1 text-xs text-muted-foreground">
          {errors.slice(0, 8).map((message, index) => (
            <li key={index} className="truncate">
              {message}
            </li>
          ))}
        </ul>
      ) : null}
    </section>
  );
}
