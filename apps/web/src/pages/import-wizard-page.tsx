import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Plural, Trans, useLingui } from "@lingui/react/macro";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { DownloadIcon, XIcon } from "lucide-react";
import { useNavigate } from "react-router-dom";

import { errorMessage } from "@/api/client";
import {
  commitImport,
  fetchImportJob,
  fetchImportJobs,
  importSourcesQuery,
  startImportJob,
  stopImportJob,
} from "@/api/imports";
import { useInvalidateEntityData } from "@/api/invalidate-entity-data";
import { configQuery, providerCatalogQuery } from "@/api/queries";
import { ConfigureStep } from "@/components/import/configure-step";
import { isActive } from "@/components/import/import-labels";
import { JobProgress, OptionToggles } from "@/components/import/import-status";
import { PlanReview } from "@/components/import/plan-review";
import { AppFrame } from "@/components/layout/app-frame";
import { PageContainer } from "@/components/layout/page-container";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { CONTENT_WRITES_DISABLED, useCapabilities } from "@/lib/capabilities";
import { useLanguagePreference } from "@/lib/language";
import type {
  ImportJob,
  ImportPlanBucket,
} from "@/types/api";

export function ImportWizardPage() {
  const { t } = useLingui();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const capabilities = useCapabilities();
  const contentWritable = capabilities.contentWritable;
  const config = useQuery(configQuery());
  const sources = useQuery(importSourcesQuery());
  const providerCatalog = useQuery(providerCatalogQuery());
  const invalidateEntityData = useInvalidateEntityData();
  // The viewer's language preference — sent with the job so sources that carry
  // multiple languages localize the review-list titles (see the `start` mutation).
  const language = useLanguagePreference();

  const [sourceId, setSourceId] = useState("");
  const [username, setUsername] = useState("");
  const [csvText, setCsvText] = useState("");
  const [jobId, setJobId] = useState<string>();
  const [bucketTypes, setBucketTypes] = useState<Record<string, string>>({});
  const [skip, setSkip] = useState<ReadonlySet<number>>(new Set());
  const [options, setOptions] = useState({
    importUserData: true,
    importEpisodes: true,
    markProgress: true,
  });

  const source = sources.data?.sources.find((item) => item.id === sourceId);

  const job = useQuery({
    queryKey: ["importJob", jobId],
    queryFn: () => fetchImportJob(jobId as string),
    enabled: Boolean(jobId),
    refetchInterval: (query) =>
      isActive((query.state.data as ImportJob | undefined)?.status) ? 1000 : false,
  });
  const status = job.data?.status;
  const plan = job.data?.plan;

  useEffect(() => {
    if (status === "completed") void invalidateEntityData();
  }, [status, invalidateEntityData]);

  // A reload drops the in-memory `jobId`, so on first mount we ask the server
  // whether a job is still running (or awaiting review) and re-adopt it. The ref
  // guards against re-adopting after the user explicitly hit "Start over" (which
  // clears `jobId` and would otherwise re-enable this query mid-session).
  const recoveredRef = useRef(false);
  const runningJobs = useQuery({
    queryKey: ["importJobs"],
    queryFn: () => fetchImportJobs(),
    enabled: !jobId,
    refetchOnWindowFocus: false,
  });
  useEffect(() => {
    if (recoveredRef.current || jobId || !runningJobs.data) return;
    recoveredRef.current = true;
    const resumable = runningJobs.data.jobs.find(
      (item) => isActive(item.status) || item.status === "planned",
    );
    if (resumable) {
      queryClient.setQueryData(["importJob", resumable.id], resumable);
      setJobId(resumable.id);
    }
  }, [jobId, runningJobs.data, queryClient]);

  const typeLabels = useMemo(() => {
    const labels = new Map<string, string>();
    for (const type of config.data?.types ?? []) labels.set(type.id, type.label);
    return labels;
  }, [config.data]);
  const providerLabels = useMemo(() => {
    const labels = new Map<string, string>();
    for (const item of providerCatalog.data?.providers ?? []) labels.set(item.id, item.label);
    return labels;
  }, [providerCatalog.data]);

  function effectiveType(bucket: ImportPlanBucket): string | undefined {
    return bucketTypes[bucket.bucket] ?? bucket.selectedType ?? bucket.candidateTypes[0];
  }

  function resetToConfigure() {
    setJobId(undefined);
    setCsvText("");
    setBucketTypes({});
    setSkip(new Set());
  }

  // Stable, so the memoized plan rows don't all re-render on every toggle —
  // plans from a MAL/Goodreads export routinely run to thousands of rows.
  const onToggleSkip = useCallback((index: number) => {
    setSkip((prev) => {
      const next = new Set(prev);
      if (next.has(index)) next.delete(index);
      else next.add(index);
      return next;
    });
  }, []);

  const start = useMutation({
    mutationFn: () => {
      // `language` localizes review-list display titles where the source
      // distinguishes languages (e.g. Bangumi's name/name_cn), mirroring
      // Quick Capture's external search.
      const input = {
        ...(source?.input === "csv" ? { csvText } : { username }),
        language,
      };
      return startImportJob({ source: sourceId, input });
    },
    onSuccess: (created) => {
      setBucketTypes({});
      setSkip(new Set());
      setJobId(created.id);
    },
  });

  // Seed the returned job into the poll cache so `refetchInterval` resumes: the
  // job sits at `planned` with polling stopped, and commit/cancel move it to an
  // active/terminal state the poll must pick up.
  const applyJob = (updated: ImportJob) => queryClient.setQueryData(["importJob", jobId], updated);

  const commit = useMutation({
    mutationFn: () => {
      const types: Record<string, string> = {};
      for (const bucket of plan?.buckets ?? []) {
        const chosen = effectiveType(bucket);
        if (chosen) types[bucket.bucket] = chosen;
      }
      const decisions = [...skip].map((index) => ({ index, action: "skip" as const }));
      return commitImport(jobId as string, { types, decisions, options });
    },
    onSuccess: applyJob,
  });

  const cancel = useMutation({
    mutationFn: () => stopImportJob(jobId as string),
    onSuccess: applyJob,
  });

  const queryError = config.error ?? sources.error ?? capabilities.error;

  const canStart =
    Boolean(source) &&
    (source?.input === "csv" ? csvText.trim().length > 0 : username.trim().length > 0) &&
    (source?.available ?? false);

  const willCreate = useMemo(
    () => (plan?.items ?? []).filter((item) => item.state === "willCreate"),
    [plan],
  );
  const toCreate = willCreate.filter((item) => !skip.has(item.index)).length;

  return (
    <AppFrame error={queryError ? errorMessage(queryError) : undefined}>
      <PageContainer>
        <header className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <h1 className="truncate text-base font-semibold">
              <Trans>Import</Trans>
            </h1>
          </div>
          {jobId ? (
            <Button variant="outline" onClick={resetToConfigure}>
              <Trans>Start over</Trans>
            </Button>
          ) : null}
        </header>

        {!contentWritable ? (
          <Alert>
            {CONTENT_WRITES_DISABLED} <Trans>Importing creates files, so it is unavailable here.</Trans>
          </Alert>
        ) : !jobId ? (
          <ConfigureStep
            sources={sources.data?.sources ?? []}
            sourceId={sourceId}
            onSelectSource={(id) => {
              setSourceId(id);
              setUsername("");
              setCsvText("");
            }}
            source={source}
            username={username}
            onUsername={setUsername}
            onCsvText={setCsvText}
            canStart={canStart}
            starting={start.isPending}
            onStart={() => start.mutate()}
          />
        ) : !job.data ? (
          <p className="rounded-md border border-dashed p-6 text-center text-sm text-muted-foreground">
            <Trans>Loading…</Trans>
          </p>
        ) : status === "failed" ? (
          <section className="rounded-md border p-4">
            <p className="text-sm text-destructive">
              {job.data.errors?.[0] ?? t`The import failed.`}
            </p>
            <Button className="mt-3" variant="outline" onClick={resetToConfigure}>
              <Trans>Start over</Trans>
            </Button>
          </section>
        ) : isActive(status) && status !== "committing" ? (
          <section className="rounded-md border border-dashed p-6 text-center text-sm text-muted-foreground">
            <Trans>Fetching your library from {source?.label ?? sourceId}…</Trans>
            <div className="mt-3">
              <Button variant="outline" size="sm" onClick={() => cancel.mutate()} disabled={cancel.isPending}>
                <Trans>Cancel</Trans>
              </Button>
            </div>
          </section>
        ) : status === "planned" ? (
          <>
            <PlanReview
              plan={plan}
              onBucketType={(bucket, type) => setBucketTypes((prev) => ({ ...prev, [bucket]: type }))}
              effectiveType={effectiveType}
              skip={skip}
              onToggleSkip={onToggleSkip}
              typeLabels={typeLabels}
              providerLabels={providerLabels}
            />
            <section className="sticky bottom-0 flex flex-wrap items-center gap-3 rounded-md border bg-background p-3">
              <OptionToggles options={options} onChange={setOptions} />
              <div className="ml-auto flex items-center gap-2">
                <Button
                  onClick={() => commit.mutate()}
                  disabled={toCreate === 0 || commit.isPending}
                >
                  <DownloadIcon data-icon="inline-start" />
                  <Plural value={toCreate} one="Import # item" other="Import # items" />
                </Button>
              </div>
            </section>
          </>
        ) : (
          <>
            <JobProgress job={job.data} />
            {status === "committing" ? (
              <Button variant="outline" size="sm" onClick={() => cancel.mutate()} disabled={cancel.isPending}>
                <XIcon data-icon="inline-start" />
                <Trans>Cancel</Trans>
              </Button>
            ) : (
              <div className="flex flex-wrap gap-2">
                <Button onClick={() => navigate("/library")}>
                  <Trans>Go to library</Trans>
                </Button>
                <Button variant="outline" onClick={resetToConfigure}>
                  <Trans>Import another</Trans>
                </Button>
              </div>
            )}
          </>
        )}
      </PageContainer>
    </AppFrame>
  );
}
