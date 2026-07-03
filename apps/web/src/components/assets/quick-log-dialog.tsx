import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { PencilLineIcon, XIcon } from "lucide-react";
import { toast } from "sonner";

import { errorMessage } from "@/api/client";
import { useInvalidateEntityData } from "@/api/invalidate-entity-data";
import { postLogActivity } from "@/api/log";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { todayLocal } from "@/lib/date";
import { cn } from "@/lib/utils";
import type { LogActivityRequest } from "@/types/api";

type Kind = "progress" | "started" | "completed";

const KIND_LABELS: Record<Kind, string> = {
  progress: "Progress",
  started: "Started",
  completed: "Completed",
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
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  entityId: string;
  revision: string;
  kinds?: Kind[];
  /// Resolves a frontmatter field name to its schema display label (for the
  /// "Stamps …" preview line). Falls back to the raw name when absent.
  fieldLabel?: (field: string) => string;
}) {
  const invalidateEntityData = useInvalidateEntityData();
  const [date, setDate] = useState(todayLocal());
  const [kind, setKind] = useState<Kind>("progress");
  const [note, setNote] = useState("");
  const [saving, setSaving] = useState(false);

  // Keep the picker in range if the entity (and so its supported kinds) changes
  // while this dialog instance is reused.
  const activeKind = kinds.includes(kind) ? kind : "progress";

  const request: LogActivityRequest = {
    date,
    kind: activeKind,
    revision,
    ...(note.trim() ? { note: note.trim() } : {}),
  };

  const preview = useQuery({
    queryKey: ["logPreview", entityId, date, activeKind, note],
    queryFn: () => postLogActivity(entityId, request, true),
    enabled: open && Boolean(date.trim()),
    retry: false,
  });

  async function submit() {
    setSaving(true);
    try {
      await postLogActivity(entityId, request);
      await invalidateEntityData();
      onOpenChange(false);
    } catch (logError) {
      toast.error(errorMessage(logError));
    } finally {
      setSaving(false);
    }
  }

  const preventReason = preview.error ? errorMessage(preview.error) : undefined;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Log activity</DialogTitle>
          <DialogDescription>
            Record what you did and when. Pick an earlier date to log something from a past day.
          </DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-4">
          {kinds.length > 1 ? (
            <div className="flex flex-col gap-1.5">
              <span className="text-sm font-medium">Activity</span>
              <div className="flex gap-1 rounded-lg bg-muted p-1">
                {kinds.map((option) => (
                  <button
                    key={option}
                    type="button"
                    onClick={() => setKind(option)}
                    className={cn(
                      "flex-1 rounded-md px-3 py-1.5 text-sm font-medium transition-colors",
                      activeKind === option
                        ? "bg-background text-foreground shadow-sm"
                        : "text-muted-foreground hover:text-foreground",
                    )}
                  >
                    {KIND_LABELS[option]}
                  </button>
                ))}
              </div>
            </div>
          ) : null}

          <label className="flex flex-col gap-1.5 text-sm font-medium">
            Date
            <Input type="date" value={date} onChange={(event) => setDate(event.target.value)} />
          </label>

          <label className="flex flex-col gap-1.5 text-sm font-medium">
            Note <span className="font-normal text-muted-foreground">(optional)</span>
            <Input
              value={note}
              placeholder="Anything worth remembering — e.g. an episode number"
              onChange={(event) => setNote(event.target.value)}
            />
          </label>

          <LogPreview
            reason={preventReason}
            data={preview.data}
            pending={preview.isFetching}
            fieldLabel={fieldLabel}
          />
        </div>

        <DialogFooter>
          <Button type="button" variant="outline" onClick={() => onOpenChange(false)} disabled={saving}>
            <XIcon data-icon="inline-start" />
            Cancel
          </Button>
          <Button type="button" onClick={() => void submit()} disabled={saving || Boolean(preventReason)}>
            <PencilLineIcon data-icon="inline-start" />
            {saving ? "Logging…" : "Log"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function LogPreview({
  reason,
  data,
  pending,
  fieldLabel,
}: {
  reason?: string;
  data?: Awaited<ReturnType<typeof postLogActivity>>;
  pending: boolean;
  fieldLabel?: (field: string) => string;
}) {
  if (reason) {
    return <p className="rounded-md bg-muted p-2 text-xs text-muted-foreground">{reason}</p>;
  }
  if (!data) {
    return (
      <p className="rounded-md bg-muted p-2 text-xs text-muted-foreground">
        {pending ? "Previewing…" : "—"}
      </p>
    );
  }
  return (
    <div className="flex flex-col gap-1 rounded-md bg-muted p-2 text-xs">
      {data.line ? (
        <code className="break-words font-mono text-foreground">{data.line}</code>
      ) : (
        <span className="text-muted-foreground">Nothing to log for this type.</span>
      )}
      {data.notePath ? (
        <span className="text-muted-foreground">
          → {data.notePath}
          {data.lineAlreadyPresent ? " (already logged)" : data.noteWillBeCreated ? " (new note)" : ""}
        </span>
      ) : null}
      {data.willStampDate ? (
        <span className="text-muted-foreground">
          Stamps {fieldLabel?.(data.willStampDate.field) ?? data.willStampDate.field} ={" "}
          {data.willStampDate.value}
        </span>
      ) : null}
      {data.willFlipStatus ? (
        <span className="text-muted-foreground">Marks as {data.willFlipStatus.value}</span>
      ) : null}
    </div>
  );
}
