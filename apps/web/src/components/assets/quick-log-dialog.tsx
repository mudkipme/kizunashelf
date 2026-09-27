import type { MessageDescriptor } from "@lingui/core";
import { msg } from "@lingui/core/macro";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { PencilLineIcon, XIcon } from "lucide-react";
import { useEffect, useState } from "react";

import { errorMessage, isConflictError } from "@/api/client";
import { useInvalidateEntityData } from "@/api/invalidate-entity-data";
import { postLogActivity } from "@/api/log";
import { queryKeys, entityQuery } from "@/api/queries";
import { SaveFailure } from "@/components/save-failure";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { todayLocal } from "@/lib/date";
import { cn } from "@/lib/utils";
import type { LogActivityRequest, LogKind } from "@/types/api";

type Kind = LogKind;

const KIND_LABELS: Record<Kind, MessageDescriptor> = {
  progress: msg`Activity`,
  started: msg`Started`,
  completed: msg`Completed`,
};

/// Records an activity for an entity through `/log`: a daily-note line, and — for a
/// started/completed log — a frontmatter date stamp. A live preview shows what it
/// will record before you commit, and the date is editable so you can record
/// something you did on an earlier day. `kinds` is the set of activities this type
/// supports, derived by the core — when there's only one, the picker is hidden. Episode check-offs are a
/// separate flow (the episode list) and never part of logging.
export function QuickLogDialog({
  open,
  onOpenChange,
  entityId,
  revision,
  kinds = ["progress"],
  initialKind = "progress",
  writesNote = true,
  fieldLabel,
  onCompleted,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  entityId: string;
  revision: string;
  kinds?: Kind[];
  initialKind?: Kind;
  writesNote?: boolean;
  /// Resolves a frontmatter field name to its schema display label (for the
  /// "Stamps …" preview line). Falls back to the raw name when absent.
  fieldLabel?: (field: string) => string;
  onCompleted?: () => void;
}) {
  const { t, i18n } = useLingui();
  const invalidateEntityData = useInvalidateEntityData();
  const [date, setDate] = useState(todayLocal());
  const [kind, setKind] = useState<Kind>(initialKind);
  const [note, setNote] = useState("");
  const [saving, setSaving] = useState(false);
  const queryClient = useQueryClient();
  const [draftRevision, setDraftRevision] = useState(revision);
  const [saveError, setSaveError] = useState<unknown>();
  const [recovering, setRecovering] = useState(false);
  const [conflict, setConflict] = useState(false);
  // A background refresh must not silently authorize writing over a newer entity.
  useEffect(() => {
    if (!open) {
      setDraftRevision(revision);
      setConflict(false);
      setSaveError(undefined);
    }
  }, [open, revision]);

  async function reloadLatest() {
    if (recovering) return;
    setRecovering(true);
    try {
      const latest = await queryClient.fetchQuery({ ...entityQuery(entityId), staleTime: 0 });
      setDraftRevision(latest.entity.revision);
      setConflict(false);
      setSaveError(undefined);
    } catch (error) {
      setSaveError(error);
    } finally {
      setRecovering(false);
    }
  }

  // Keep the picker in range if the entity (and so its supported kinds) changes
  // while this dialog instance is reused.
  const activeKind = kinds.includes(kind) ? kind : kinds[0];

  const request: LogActivityRequest = {
    date,
    kind: activeKind,
    revision: draftRevision,
    ...(writesNote && note.trim() ? { note: note.trim() } : {}),
  };

  const preview = useQuery({
    queryKey: [
      ...queryKeys.logPreview(entityId, date, activeKind ?? "", note),
      draftRevision,
      writesNote,
    ],
    queryFn: () => postLogActivity(entityId, request, true),
    enabled: open && Boolean(activeKind) && Boolean(date.trim()),
    retry: false,
  });

  useEffect(() => {
    if (isConflictError(preview.error)) setConflict(true);
  }, [preview.error]);

  async function submit() {
    if (
      !activeKind ||
      saving ||
      recovering ||
      preview.isFetching ||
      preview.error ||
      conflict ||
      !date.trim()
    )
      return;
    setSaving(true);
    setSaveError(undefined);
    try {
      await postLogActivity(entityId, request);
      await invalidateEntityData();
      onOpenChange(false);
      if (activeKind === "completed") onCompleted?.();
    } catch (logError) {
      setSaveError(logError);
      if (isConflictError(logError)) setConflict(true);
    } finally {
      setSaving(false);
    }
  }

  const preventReason = preview.error ? errorMessage(preview.error) : undefined;

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!saving && !recovering) onOpenChange(next);
      }}
    >
      <DialogContent aria-describedby={undefined} className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            {activeKind === "started" ? (
              <Trans>Start</Trans>
            ) : activeKind === "completed" ? (
              <Trans>Finish</Trans>
            ) : (
              <Trans>Log activity</Trans>
            )}
          </DialogTitle>
        </DialogHeader>

        <div className="flex min-h-0 flex-col gap-4 max-sm:flex-1 max-sm:overflow-auto">
          {kinds.length > 1 ? (
            <div className="flex flex-col gap-1.5">
              <span className="text-sm font-medium">
                <Trans>Activity</Trans>
              </span>
              <div className="flex gap-1 rounded-lg bg-muted p-1">
                {kinds.map((option) => (
                  <button
                    key={option}
                    type="button"
                    disabled={saving || recovering}
                    aria-pressed={activeKind === option}
                    onClick={() => setKind(option)}
                    className={cn(
                      "flex-1 rounded-md px-3 py-1.5 text-sm font-medium transition-colors",
                      activeKind === option
                        ? "bg-background text-foreground shadow-sm"
                        : "text-muted-foreground hover:text-foreground",
                    )}
                  >
                    {i18n._(KIND_LABELS[option])}
                  </button>
                ))}
              </div>
            </div>
          ) : null}

          <label className="flex flex-col gap-1.5 text-sm font-medium">
            <Trans>Date</Trans>
            <Input
              disabled={saving || recovering}
              type="date"
              value={date}
              onChange={(event) => setDate(event.target.value)}
            />
          </label>

          {writesNote ? (
            <label className="flex flex-col gap-1.5 text-sm font-medium">
              <Trans>
                Note <span className="font-normal text-muted-foreground">(optional)</span>
              </Trans>
              <Input
                disabled={saving || recovering}
                value={note}
                placeholder={t`Anything worth remembering`}
                onChange={(event) => setNote(event.target.value)}
              />
            </label>
          ) : null}

          {!activeKind ? (
            <p className="text-sm text-muted-foreground">
              <Trans>No activity actions are available.</Trans>
            </p>
          ) : null}
          <SaveFailure
            error={saveError ?? preview.error}
            recover={
              conflict
                ? () => void reloadLatest()
                : preview.error
                  ? () => void preview.refetch()
                  : undefined
            }
            recovering={recovering || preview.isFetching}
          />
          <LogPreview
            data={preview.error || !activeKind ? undefined : preview.data}
            pending={preview.isFetching}
            fieldLabel={fieldLabel}
          />
        </div>

        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={saving || recovering}
          >
            <XIcon data-icon="inline-start" />
            <Trans>Cancel</Trans>
          </Button>
          <Button
            type="button"
            onClick={() => void submit()}
            disabled={
              saving ||
              recovering ||
              preview.isFetching ||
              Boolean(preventReason) ||
              conflict ||
              !activeKind ||
              !date.trim()
            }
          >
            <PencilLineIcon data-icon="inline-start" />
            {saving
              ? t`Saving…`
              : activeKind === "started"
                ? t`Start`
                : activeKind === "completed"
                  ? t`Finish`
                  : t`Log`}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function LogPreview({
  data,
  pending,
  fieldLabel,
}: {
  data?: Awaited<ReturnType<typeof postLogActivity>>;
  pending: boolean;
  fieldLabel?: (field: string) => string;
  onCompleted?: () => void;
}) {
  const { t } = useLingui();
  if (!data) {
    return pending ? <p className="text-xs text-muted-foreground">{t`Previewing…`}</p> : null;
  }
  return (
    <div className="flex flex-col gap-2 text-sm" aria-live="polite">
      {data.willFlipStatus ? (
        <p>
          <Trans>Mark as {data.willFlipStatus.value}</Trans>
        </p>
      ) : null}
      {data.willStampDate ? (
        <p>
          <Trans>
            Set {fieldLabel?.(data.willStampDate.field) ?? data.willStampDate.field} to{" "}
            {data.willStampDate.value}
          </Trans>
        </p>
      ) : null}
      {data.line ? (
        <>
          <p>
            {data.lineAlreadyPresent ? (
              <Trans>Already in daily note</Trans>
            ) : (
              <Trans>Add to daily note</Trans>
            )}
          </p>
          <details className="text-xs text-muted-foreground">
            <summary className="cursor-pointer">
              <Trans>Journal preview</Trans>
            </summary>
            <div className="mt-2 space-y-1 rounded-md bg-muted p-2">
              <p className="break-all">{data.notePath}</p>
              <code className="block break-words whitespace-pre-wrap text-foreground">
                {data.line}
              </code>
            </div>
          </details>
        </>
      ) : null}
    </div>
  );
}
