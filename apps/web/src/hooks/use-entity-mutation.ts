import { useCallback, useState } from "react";
import { toast } from "sonner";

import { errorMessage, isAbortError, isConflictError } from "@/api/client";

/** Shown after a 409 on a detail-page action: the page auto-refetches and the
 * user just retries. */
export const ENTITY_CONFLICT_MESSAGE =
  "This entity changed on disk since it was loaded. Reloaded the latest version — please try again.";

/** Shown after a 409 while editing: the in-progress draft is kept and the user
 * reloads the latest version manually (no auto-refetch that would clobber it). */
export const ENTITY_EDIT_CONFLICT_MESSAGE =
  "This entity changed on disk since you opened it. Your edits are kept here — reload the latest version, then reapply them.";

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
      toast.error(options?.conflictMessage ?? ENTITY_CONFLICT_MESSAGE);
    }
  } else {
    toast.error(errorMessage(error));
  }
}

/**
 * The shared envelope for entity write actions: flips a `saving` flag, runs the
 * action, and routes failures through {@link reportEntityError} (a toast).
 *
 * The action owns its own success path (cache invalidation, navigation, a
 * success toast, …) so ordering stays explicit — e.g. downloading a cover before
 * invalidating. The hook only de-duplicates the `setSaving`/try/catch/finally
 * boilerplate that every write handler repeated.
 */
export function useEntityMutation() {
  const [saving, setSaving] = useState(false);

  const run = useCallback(async (action: () => Promise<void>, options?: ReportOptions) => {
    setSaving(true);
    try {
      await action();
    } catch (caught) {
      reportEntityError(caught, options);
    } finally {
      setSaving(false);
    }
  }, []);

  return { saving, run };
}
