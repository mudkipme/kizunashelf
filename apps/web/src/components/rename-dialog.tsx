import { Trans, useLingui } from "@lingui/react/macro";
import { CheckIcon, XIcon } from "lucide-react";
import { useEffect, useId, useState } from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { basenameValidationError, normalizeBasename } from "@/lib/basename";

export function RenameDialog({
  open,
  onOpenChange,
  currentName,
  title,
  label,
  saving,
  disabled,
  onRename,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  currentName: string;
  title: string;
  label?: string;
  saving: boolean;
  disabled: boolean;
  onRename: (name: string) => void;
}) {
  const { t } = useLingui();
  const errorId = useId();
  const [name, setName] = useState(currentName);
  useEffect(() => {
    if (open) setName(currentName);
  }, [open, currentName]);

  const normalized = normalizeBasename(name);
  const validationError = name.trim() ? basenameValidationError(normalized) : undefined;
  const unchanged = normalized === currentName;
  const cannotSubmit = disabled || saving || Boolean(validationError) || unchanged || !name.trim();

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!saving) onOpenChange(next);
      }}
    >
      <DialogContent aria-describedby={undefined}>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            if (cannotSubmit) return;
            onRename(normalized);
          }}
          className="flex flex-col gap-2"
        >
          <label className="text-sm font-medium">
            {label ?? t`Name`}
            <Input
              value={name}
              onChange={(event) => setName(event.target.value)}
              disabled={disabled || saving}
              aria-invalid={Boolean(validationError)}
              aria-describedby={validationError ? errorId : undefined}
              autoFocus
            />
          </label>
          {validationError ? (
            <p id={errorId} className="text-xs text-destructive">
              {validationError}
            </p>
          ) : null}
          <DialogFooter className="mt-2">
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
              disabled={saving}
            >
              <XIcon data-icon="inline-start" />
              <Trans>Cancel</Trans>
            </Button>
            <Button type="submit" disabled={cannotSubmit}>
              <CheckIcon data-icon="inline-start" />
              {saving ? <Trans>Renaming…</Trans> : <Trans>Rename</Trans>}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
