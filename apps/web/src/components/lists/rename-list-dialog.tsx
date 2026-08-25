//! Renaming a list, which renames its Markdown file — so the new name goes
//! through the same basename validation a file gets everywhere else.

import { useEffect, useState } from "react";
import { Trans } from "@lingui/react/macro";
import { CheckIcon, XIcon } from "lucide-react";

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

export function RenameListDialog({
  open,
  onOpenChange,
  currentName,
  saving,
  disabled,
  onRename,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  currentName: string;
  saving: boolean;
  disabled: boolean;
  onRename: (name: string) => void;
}) {
  const [name, setName] = useState(currentName);
  useEffect(() => {
    if (open) setName(currentName);
  }, [open, currentName]);

  const normalized = normalizeBasename(name);
  const validationError = name.trim() ? basenameValidationError(normalized) : undefined;
  const unchanged = normalized === currentName;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            <Trans>Rename list</Trans>
          </DialogTitle>
          <DialogDescription>
            <Trans>Changes the list's name.</Trans>
          </DialogDescription>
        </DialogHeader>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            if (!name.trim() || validationError || unchanged) return;
            onRename(normalized);
          }}
          className="flex flex-col gap-2"
        >
          <label className="text-sm font-medium">
            <Trans>Name</Trans>
            <Input
              value={name}
              onChange={(event) => setName(event.target.value)}
              disabled={disabled || saving}
              aria-invalid={Boolean(validationError)}
            />
          </label>
          {validationError ? <p className="text-xs text-destructive">{validationError}</p> : null}
          <DialogFooter className="mt-2">
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)} disabled={saving}>
              <XIcon data-icon="inline-start" />
              <Trans>Cancel</Trans>
            </Button>
            <Button type="submit" disabled={disabled || saving || Boolean(validationError) || unchanged || !name.trim()}>
              <CheckIcon data-icon="inline-start" />
              {saving ? <Trans>Renaming…</Trans> : <Trans>Rename</Trans>}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
