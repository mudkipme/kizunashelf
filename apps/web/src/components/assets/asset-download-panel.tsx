import { Trans, useLingui } from "@lingui/react/macro";
import { useMutation, useQuery } from "@tanstack/react-query";
import { DownloadIcon, XIcon } from "lucide-react";
import { useEffect, useState } from "react";

import { errorMessage } from "@/api/client";
import { fetchAssetJob, startAssetJob, stopAssetJob } from "@/api/entities";
import { useInvalidateEntityData } from "@/api/invalidate-entity-data";
import { configQuery, queryKeys } from "@/api/queries";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";
import { useCapabilities } from "@/lib/capabilities";
import type { AssetDownloadJob } from "@/types/api";

const ALL_TYPES = "__all__";

function isRunning(job?: AssetDownloadJob) {
  return job?.status === "queued" || job?.status === "running";
}

export function AssetDownloadPanel() {
  const { t } = useLingui();
  const capabilities = useCapabilities();
  const config = useQuery(configQuery());
  const invalidateEntityData = useInvalidateEntityData();
  const [selectedType, setSelectedType] = useState(ALL_TYPES);
  const [jobId, setJobId] = useState<string>();
  const [error, setError] = useState<string>();

  const job = useQuery({
    queryKey: queryKeys.assetJob(jobId),
    queryFn: ({ signal }) => fetchAssetJob(jobId as string, { signal }),
    enabled: Boolean(jobId),
    refetchInterval: (query) =>
      isRunning(query.state.data as AssetDownloadJob | undefined) ? 1000 : false,
  });

  const status = job.data?.status;
  useEffect(() => {
    if (status === "completed" || status === "cancelled") {
      void invalidateEntityData();
    }
  }, [status, invalidateEntityData]);

  const start = useMutation({
    mutationFn: () =>
      startAssetJob({ entityType: selectedType === ALL_TYPES ? undefined : selectedType }),
    onSuccess: (created) => {
      setError(undefined);
      setJobId(created.id);
    },
    onError: (mutationError) => setError(errorMessage(mutationError)),
  });

  const cancel = useMutation({
    mutationFn: () => stopAssetJob(jobId as string),
    onError: (mutationError) => setError(errorMessage(mutationError)),
  });

  if (!capabilities.assetDownloadEnabled) return null;

  const current = job.data;
  const running = isRunning(current);

  return (
    <section className="rounded-md border p-3">
      {/* Centred, not bottom-aligned: this row pairs a plain heading with a
          control group, so there is no stacked label whose input needs to line
          up with the controls beside it. */}
      <div className="flex flex-wrap items-center gap-2">
        <div className="min-w-0">
          <h2 className="text-sm font-semibold">
            <Trans>Download remote covers</Trans>
          </h2>
        </div>
        <div className="ml-auto flex items-center gap-2">
          <Select
            value={selectedType}
            onChange={(event) => setSelectedType(event.target.value)}
            disabled={running}
            aria-label={t`Download scope`}
          >
            <option value={ALL_TYPES}>{t`All types`}</option>
            {(config.data?.types ?? []).map((type) => (
              <option key={type.id} value={type.id}>
                {type.label}
              </option>
            ))}
          </Select>
          <Button
            type="button"
            onClick={() => start.mutate()}
            disabled={running || start.isPending}
          >
            <DownloadIcon data-icon="inline-start" />
            {running ? <Trans>Running…</Trans> : <Trans>Start</Trans>}
          </Button>
          {running ? (
            <Button
              type="button"
              variant="outline"
              onClick={() => cancel.mutate()}
              disabled={cancel.isPending}
            >
              <XIcon data-icon="inline-start" />
              <Trans>Cancel</Trans>
            </Button>
          ) : null}
        </div>
      </div>
      {error ? <p className="mt-2 text-xs text-destructive">{error}</p> : null}
      {current ? <JobProgress job={current} /> : null}
    </section>
  );
}

function JobProgress({ job }: { job: AssetDownloadJob }) {
  const percent = job.total > 0 ? Math.round((job.processed / job.total) * 100) : 100;
  const errors = job.errors ?? [];
  return (
    <div className="mt-3">
      <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
        <Badge variant="secondary">{job.scope}</Badge>
        <span className="tabular-nums">
          <Trans>
            {job.processed} / {job.total} processed
          </Trans>
        </span>
        <Badge variant="outline">
          <Trans>{job.downloaded} downloaded</Trans>
        </Badge>
        {job.failed > 0 ? (
          <span className="text-destructive">
            <Trans>{job.failed} failed</Trans>
          </span>
        ) : null}
        {job.skipped > 0 ? (
          <span>
            <Trans>{job.skipped} skipped</Trans>
          </span>
        ) : null}
        <span className="capitalize">{job.status}</span>
      </div>
      <div className="mt-2 h-2 rounded-sm bg-muted">
        <div className="h-2 rounded-sm bg-primary" style={{ width: `${percent}%` }} />
      </div>
      {errors.length > 0 ? (
        <ul className="mt-2 space-y-1 text-xs text-muted-foreground">
          {errors.slice(0, 5).map((item, index) => (
            <li key={`${item.entityId}-${index}`} className="truncate">
              {item.entityTitle || item.entityId}: {item.message}
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
