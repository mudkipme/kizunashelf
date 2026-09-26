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
import type { LogActivityRequest } from "@/types/api";

type Kind = "progress" | "started" | "completed";

const KIND_LABELS: Record<Kind, MessageDescriptor> = {
  progress: msg`Progress`,
  started: msg`Started`,
  completed: msg`Completed`,
};

/// Records an activity for an entity through `/log`: a daily-note line, and — for a
/// started/completed log — a frontmatter date stamp. A live preview shows what it
/// will record before you commit, and the date is editable so you can record
/// something you did on an earlier day. `kinds` is the set of activities this type
/// supports (always Progress; Started/Completed only when the schema has those date
/// roles) — when there's only one, the picker is hidden. Episode check-offs are a
/// separate flow (the episode list) and never part of logging.
export function QuickLogDialog({
  open,
  onOpenChange,
  entityId,
  revision,
  kinds = ["progress"],
  fieldLabel,
  onCompleted,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  entityId: string;
  revision: string;
  kinds?: Kind[];
  /// Resolves a frontmatter field name to its schema display label (for the
  /// "Stamps …" preview line). Falls back to the raw name when absent.
  fieldLabel?: (field: string) => string;
  onCompleted?: () => void;
}) {
  const { t, i18n } = useLingui();
  const invalidateEntityData = useInvalidateEntityData();
  const [date, setDate] = useState(todayLocal());
  const [kind, setKind] = useState<Kind>("progress");
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
  const activeKind = kinds.includes(kind) ? kind : "progress";

  const request: LogActivityRequest = {
    date,
    kind: activeKind,
    revision: draftRevision,
    ...(note.trim() ? { note: note.trim() } : {}),
  };

  const preview = useQuery({
    queryKey: [...queryKeys.logPreview(entityId, date, activeKind, note), draftRevision],
    queryFn: () => postLogActivity(entityId, request, true),
    enabled: open && Boolean(date.trim()),
    retry: false,
  });

  useEffect(() => {
    if (isConflictError(preview.error)) setConflict(true);
  }, [preview.error]);

  async function submit() {
    if (saving || recovering || preview.isFetching || preview.error || conflict || !date.trim())
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
            <Trans>Log activity</Trans>
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
            <span className="text-xs font-normal text-muted-foreground">
              <Trans>For example, an episode number.</Trans>
            </span>
          </label>

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
          <LogPreview data={preview.data} pending={preview.isFetching} fieldLabel={fieldLabel} />
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
              !date.trim()
            }
          >
            <PencilLineIcon data-icon="inline-start" />
            {saving ? t`Logging…` : t`Log`}
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
    return (
      <p className="rounded-md bg-muted p-2 text-xs text-muted-foreground">
        {pending ? t`Previewing…` : "—"}
      </p>
    );
  }
  return (
    <div className="flex flex-col gap-1 rounded-md bg-muted p-2 text-xs">
      {data.line ? (
        <code className="font-mono break-words text-foreground">{data.line}</code>
      ) : (
        <span className="text-muted-foreground">
          <Trans>Nothing to log for this type.</Trans>
        </span>
      )}
      {data.notePath ? (
        <span className="text-muted-foreground">
          → {data.notePath}
          {data.lineAlreadyPresent
            ? t` (already logged)`
            : data.noteWillBeCreated
              ? t` (new note)`
              : ""}
        </span>
      ) : null}
      {data.willStampDate ? (
        <span className="text-muted-foreground">
          <Trans>
            Stamps {fieldLabel?.(data.willStampDate.field) ?? data.willStampDate.field} ={" "}
            {data.willStampDate.value}
          </Trans>
        </span>
      ) : null}
      {data.willFlipStatus ? (
        <span className="text-muted-foreground">
          <Trans>Marks as {data.willFlipStatus.value}</Trans>
        </span>
      ) : null}
    </div>
  );
}
