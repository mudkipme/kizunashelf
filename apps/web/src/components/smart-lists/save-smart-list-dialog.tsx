import { Trans, useLingui } from "@lingui/react/macro";
import { useMutation } from "@tanstack/react-query";
import { CheckIcon, XIcon } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";

import { errorMessage } from "@/api/client";
import { useInvalidateLists } from "@/api/invalidate-lists";
import { addSmartList, saveSmartList } from "@/api/smart-lists";
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
import { basenameValidationError, normalizeBasename } from "@/lib/basename";
import type { SmartFilterGroup, SmartListView } from "@/types/api";

/// Names and saves a smart list from an already-built definition — the "save
/// what I'm browsing" flow. Browsing already *is* an unsaved smart list, so the
/// definition is written through verbatim; only the name is new. Creation is
/// two calls (create the file, then write the criteria into it) because create
/// only takes a name and scope.
export function SaveSmartListDialog({
  open,
  onOpenChange,
  definition,
  searchActive = false,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  definition: { scope?: string; filters: SmartFilterGroup; views: SmartListView[] };
  /// Whether a text search is narrowing what's on screen. A search is a way of
  /// looking at a list, not part of it, so it isn't saved — say so rather than
  /// let the saved list surprise them.
  searchActive?: boolean;
}) {
  const { t } = useLingui();
  const navigate = useNavigate();
  const invalidateLists = useInvalidateLists();
  const [name, setName] = useState("");
  const validationError = name.trim()
    ? basenameValidationError(normalizeBasename(name))
    : undefined;

  const create = useMutation({
    mutationFn: async () => {
      const created = await addSmartList({
        name: normalizeBasename(name),
        scope: definition.scope,
      });
      return saveSmartList(created.id, {
        revision: created.revision,
        scope: definition.scope,
        filters: definition.filters,
        views: definition.views,
      });
    },
    onSuccess: async (list) => {
      toast.success(t`Smart list created`);
      onOpenChange(false);
      setName("");
      await invalidateLists();
      navigate(`/lists/smart/${encodeURIComponent(list.id)}`);
    },
    onError: (error) => toast.error(errorMessage(error)),
  });

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        onOpenChange(next);
        if (!next) setName("");
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            <Trans>Save as smart list</Trans>
          </DialogTitle>
          <DialogDescription>
            {searchActive ? (
              <Trans>
                Saves the current type, criteria, and sort as a smart list. Your search isn't part
                of it.
              </Trans>
            ) : (
              <Trans>Saves the current type, criteria, and sort as a smart list.</Trans>
            )}
          </DialogDescription>
        </DialogHeader>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            if (!name.trim() || validationError) return;
            create.mutate();
          }}
          className="flex flex-col gap-2"
        >
          <label className="text-sm font-medium">
            <Trans>Name</Trans>
            <Input
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder={t`Watching now`}
              autoFocus
              aria-invalid={Boolean(validationError)}
            />
          </label>
          {validationError ? <p className="text-xs text-destructive">{validationError}</p> : null}
          <DialogFooter className="mt-2">
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
              disabled={create.isPending}
            >
              <XIcon data-icon="inline-start" />
              <Trans>Cancel</Trans>
            </Button>
            <Button
              type="submit"
              disabled={!name.trim() || Boolean(validationError) || create.isPending}
            >
              <CheckIcon data-icon="inline-start" />
              {create.isPending ? <Trans>Creating…</Trans> : <Trans>Create</Trans>}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
