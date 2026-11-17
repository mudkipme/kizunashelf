import { t } from "@lingui/core/macro";
import { useCallback, useRef, useState } from "react";
import { toast } from "sonner";

import { errorMessage, isAbortError, isConflictError } from "@/api/client";

type ReportOptions = {
  /** Extra recovery to run on a 409, e.g. refetching the entity or flagging a
   * conflict so the UI can offer a reload. */
  onConflict?: () => void;
  /** Override the default conflict message (e.g. the keep-edits variant). */
  conflictMessage?: string;
  /** Skip the conflict toast when the caller renders its own recovery UI (the
   * edit page shows an inline "reload latest" banner instead). */
  silentConflict?: boolean;
};

/**
 * Reports a failed entity write as a toast, turning a 409 (stale revision) into
 * a friendly conflict message (and running `onConflict` to recover, e.g. a
 * refetch) rather than surfacing a raw `409 …` string. Aborted requests (a
 * superseded fetch) are ignored.
 */
export function reportEntityError(error: unknown, options?: ReportOptions) {
  if (isAbortError(error)) return;
  if (isConflictError(error)) {
    options?.onConflict?.();
    if (!options?.silentConflict) {
      toast.error(
        options?.conflictMessage ??
          t`This entity changed elsewhere. Reload the latest version before trying again.`,
      );
    }
  } else {
    toast.error(errorMessage(error));
  }
}

/**
 * The single write slot for one entity: every write action on the entity page
 * runs through it, routing failures through {@link reportEntityError} (a toast).
 *
 * Each write sends the revision the page last saw, so two writes to the same
 * file in flight at once would make the second one conflict with the first.
 * `run` therefore refuses to start while another write holds the slot (checked
 * through a ref, so two clicks in the same tick can't both get in), and `saving`
 * is the one flag every write control on the page disables on.
 *
 * The action owns its own success path (cache invalidation, navigation, a
 * success toast, …) so ordering stays explicit — e.g. downloading a cover before
 * invalidating.
 */
export function useEntityMutation() {
  const [saving, setSaving] = useState(false);
  const inFlight = useRef(false);

  const run = useCallback(async (action: () => Promise<void>, options?: ReportOptions) => {
    if (inFlight.current) return;
    inFlight.current = true;
    setSaving(true);
    try {
      await action();
    } catch (caught) {
      reportEntityError(caught, options);
    } finally {
      inFlight.current = false;
      setSaving(false);
    }
  }, []);

  return { saving, run };
}
