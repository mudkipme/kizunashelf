import { useCallback, useState } from "react";

import { errorMessage, isConflictError } from "@/api/client";

/** Shown after a 409 on a detail-page action: the page auto-refetches and the
 * user just retries. */
export const ENTITY_CONFLICT_MESSAGE =
  "This entity changed on disk since it was loaded. Reloaded the latest version — please try again.";

/** Shown after a 409 while editing: the in-progress draft is kept and the user
 * reloads the latest version manually (no auto-refetch that would clobber it). */
export const ENTITY_EDIT_CONFLICT_MESSAGE =
  "This entity changed on disk since you opened it. Your edits are kept here — reload the latest version, then reapply them.";

/**
 * Reports a failed entity write into `setError`, turning a 409 (stale revision)
 * into a friendly conflict message (and running `onConflict` to recover, e.g. a
 * refetch) rather than surfacing a raw `409 …` string.
 */
export function reportEntityError(
  error: unknown,
  setError: (message?: string) => void,
  onConflict?: () => void,
  conflictMessage: string = ENTITY_CONFLICT_MESSAGE,
) {
  if (isConflictError(error)) {
    setError(conflictMessage);
    onConflict?.();
  } else {
    setError(errorMessage(error));
  }
}

type RunOptions = {
  /** Extra recovery to run on a 409, e.g. refetching the entity or flagging a
   * conflict so the UI can offer a reload. */
  onConflict?: () => void;
  /** Override the default conflict message (e.g. the keep-edits variant). */
  conflictMessage?: string;
};

/**
 * The shared envelope for entity write actions: flips a `saving` flag, clears the
 * error, runs the action, and routes failures through {@link reportEntityError}.
 *
 * The action owns its own success path (cache invalidation, navigation, …) so
 * ordering stays explicit — e.g. downloading a cover before invalidating. The hook
 * only de-duplicates the `setSaving`/try/catch(conflict)/finally boilerplate that
 * every write handler repeated.
 */
export function useEntityMutation() {
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string>();

  const run = useCallback(async (action: () => Promise<void>, options?: RunOptions) => {
    setSaving(true);
    setError(undefined);
    try {
      await action();
    } catch (caught) {
      reportEntityError(caught, setError, options?.onConflict, options?.conflictMessage);
    } finally {
      setSaving(false);
    }
  }, []);

  return { saving, error, setError, run };
}
