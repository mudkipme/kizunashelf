import { useEffect, useMemo, useRef, useState } from "react";
import type { I18n, MessageDescriptor } from "@lingui/core";
import { msg, plural } from "@lingui/core/macro";
import { Plural, Trans, useLingui } from "@lingui/react/macro";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { DownloadIcon, XIcon } from "lucide-react";
import { Link, useNavigate } from "react-router-dom";

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
import { AppFrame } from "@/components/layout/app-frame";
import { PageContainer } from "@/components/layout/page-container";
import { Alert } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { CONTENT_WRITES_DISABLED, useCapabilities } from "@/lib/capabilities";
import { useLanguagePreference } from "@/lib/language";
import type {
  ImportCanonicalStatus,
  ImportJob,
  ImportJobStatus,
  ImportPlanBucket,
  ImportPlanItem,
  ImportSourceCatalogItem,
} from "@/types/api";

function isActive(status?: ImportJobStatus) {
  return status === "queued" || status === "fetching" || status === "committing";
}

// Bangumi buckets are numeric `subject_type` codes; give them readable labels.
// Every other provider's bucket is already a word (anime, movie, book…).
const BANGUMI_BUCKETS: Record<string, MessageDescriptor> = {
  "1": msg`Book`,
  "2": msg`Anime`,
  "3": msg`Music`,
  "4": msg`Game`,
  "6": msg`Real`,
};
function bucketLabel(i18n: I18n, provider: string, bucket: string): string {
  if (provider === "bangumi" && BANGUMI_BUCKETS[bucket]) return i18n._(BANGUMI_BUCKETS[bucket]);
  return bucket || "—";
}

const STATUS_LABELS: Record<ImportCanonicalStatus, MessageDescriptor> = {
  planning: msg`Planning`,
  ongoing: msg`Ongoing`,
  paused: msg`Paused`,
  completed: msg`Completed`,
  dropped: msg`Dropped`,
};
function statusLabel(i18n: I18n, status: ImportCanonicalStatus): string {
  return STATUS_LABELS[status] ? i18n._(STATUS_LABELS[status]) : status;
}

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

  const willCreate = (plan?.items ?? []).filter((item) => item.state === "willCreate");
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
              onToggleSkip={(index) =>
                setSkip((prev) => {
                  const next = new Set(prev);
                  if (next.has(index)) next.delete(index);
                  else next.add(index);
                  return next;
                })
              }
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

function ConfigureStep({
  sources,
  sourceId,
  onSelectSource,
  source,
  username,
  onUsername,
  onCsvText,
  canStart,
  starting,
  onStart,
}: {
  sources: ImportSourceCatalogItem[];
  sourceId: string;
  onSelectSource: (id: string) => void;
  source: ImportSourceCatalogItem | undefined;
  username: string;
  onUsername: (value: string) => void;
  onCsvText: (value: string) => void;
  canStart: boolean;
  starting: boolean;
  onStart: () => void;
}) {
  return (
    <section className="flex flex-col gap-4 rounded-md border p-4">
      <div className="flex flex-col gap-2">
        <span className="text-sm font-medium">
          <Trans>Source</Trans>
        </span>
        <ul className="grid gap-2 sm:grid-cols-2">
          {sources.map((item) => {
            const selected = item.id === sourceId;
            const disabled = !item.available;
            return (
              <li key={item.id}>
                <button
                  type="button"
                  onClick={() => onSelectSource(item.id)}
                  disabled={disabled}
                  className={[
                    "flex w-full flex-col items-start gap-0.5 rounded-md border p-3 text-left transition-colors",
                    selected ? "border-primary ring-1 ring-primary" : "hover:bg-accent",
                    disabled ? "opacity-60" : "",
                  ].join(" ")}
                >
                  <span className="font-medium">{item.label}</span>
                  <span className="text-xs text-muted-foreground">
                    {item.input === "csv" ? <Trans>CSV export</Trans> : <Trans>Public profile</Trans>}
                  </span>
                  {disabled && item.unavailableReason ? (
                    <span className="mt-0.5 text-xs text-amber-700 dark:text-amber-400">
                      {item.unavailableReason}
                    </span>
                  ) : null}
                </button>
              </li>
            );
          })}
        </ul>
      </div>

      {source ? (
        <div className="flex flex-col gap-2">
          {source.input === "csv" ? (
            <CsvFileInput
              key={source.id}
              label={source.inputLabel}
              onCsvText={onCsvText}
            />
          ) : (
            <label className="flex max-w-sm flex-col gap-1 text-sm font-medium">
              {source.inputLabel}
              <Input
                value={username}
                onChange={(event) => onUsername(event.target.value)}
                placeholder={source.inputLabel}
                autoFocus
              />
            </label>
          )}
          <p className="text-xs text-muted-foreground">
            {source.input === "csv" ? (
              <Trans>The file is read in your browser. Nothing is written until you review the plan.</Trans>
            ) : (
              <Trans>Only public profiles are supported. Nothing is written until you review the plan.</Trans>
            )}
          </p>
          <div>
            <Button onClick={onStart} disabled={!canStart || starting}>
              {starting ? <Trans>Starting…</Trans> : <Trans>Fetch & plan</Trans>}
            </Button>
          </div>
        </div>
      ) : null}
    </section>
  );
}

function CsvFileInput({ label, onCsvText }: { label: string; onCsvText: (value: string) => void }) {
  const [readFailed, setReadFailed] = useState(false);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  async function handleFile(fileInput: HTMLInputElement) {
    const file = fileInput.files?.[0];
    setReadFailed(false);
    onCsvText("");
    if (!file) return;

    try {
      const text = await file.text();
      if (mountedRef.current && fileInput.files?.[0] === file) onCsvText(text);
    } catch {
      if (mountedRef.current && fileInput.files?.[0] === file) setReadFailed(true);
    }
  }

  return (
    <label className="flex max-w-lg flex-col gap-1 text-sm font-medium">
      {label}
      <Input
        type="file"
        accept=".csv,text/csv"
        aria-invalid={readFailed || undefined}
        onChange={(event) => void handleFile(event.currentTarget)}
      />
      {readFailed ? (
        <span className="text-xs font-normal text-destructive" role="alert">
          <Trans>Could not read this CSV file.</Trans>
        </span>
      ) : null}
    </label>
  );
}

function PlanReview({
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
  if (!plan) return null;
  const items = plan.items;
  const counts = {
    willCreate: items.filter((item) => item.state === "willCreate").length,
    exists: items.filter((item) => item.state === "exists").length,
    needsReview: items.filter((item) => item.state === "needsReview").length,
  };
  const ambiguous = plan.buckets.filter((bucket) => bucket.candidateTypes.length > 1);
  const unmatched = plan.buckets.filter((bucket) => bucket.candidateTypes.length === 0);
  const unmatchedLabels = unmatched
    .map((bucket) => `${providerLabels.get(bucket.provider) ?? bucket.provider} ${bucketLabel(i18n, bucket.provider, bucket.bucket)}`)
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
          <span>· <Plural value={counts.exists} one="# already in library" other="# already in library" /></span>
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
            No entity type maps {unmatchedLabels}. Add an external-reference field for it in Settings to
            import these.
          </Trans>
        </div>
      ) : null}

      <ul className="flex flex-col gap-1.5">
        {items.map((item) => (
          <PlanItemRow
            key={item.index}
            item={item}
            skipped={skip.has(item.index)}
            onToggleSkip={() => onToggleSkip(item.index)}
            providerLabels={providerLabels}
          />
        ))}
      </ul>
    </section>
  );
}

function PlanItemRow({
  item,
  skipped,
  onToggleSkip,
  providerLabels,
}: {
  item: ImportPlanItem;
  skipped: boolean;
  onToggleSkip: () => void;
  providerLabels: Map<string, string>;
}) {
  const { t, i18n } = useLingui();
  const creatable = item.state === "willCreate";
  return (
    <li
      className={[
        "flex items-start gap-3 rounded-md border p-2.5 text-sm",
        skipped ? "opacity-50" : "",
      ].join(" ")}
    >
      {creatable ? (
        <input
          type="checkbox"
          checked={!skipped}
          onChange={onToggleSkip}
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
            {providerLabels.get(item.provider) ?? item.provider ?? t`no source`}
          </span>
          <span className="rounded border px-1.5 py-0.5">
            {bucketLabel(i18n, item.provider, item.bucket)}
          </span>
          <UserDataSummary item={item} />
        </div>
      </div>
    </li>
  );
}

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

function reviewReasonLabel(reason: ImportPlanItem["reviewReason"]): MessageDescriptor {
  switch (reason) {
    case "noSupportedId":
      return msg`No matched source`;
    case "noTypeMatch":
      return msg`No matching type`;
    case "providerUnavailable":
      return msg`Source unavailable`;
    default:
      return msg`Needs review`;
  }
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

function OptionToggles({
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

function JobProgress({ job }: { job: ImportJob }) {
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
