import { useEffect } from "react";

/**
 * Warn the user before they close/reload the tab while `dirty` is true.
 *
 * Only the native `beforeunload` prompt is wired here (tab close, reload,
 * external navigation). In-app concerns (e.g. switching editors) are handled by
 * the caller, which already knows the dirty state. The listener is only attached
 * while `dirty` is true, so a clean editor never interferes with normal closing.
 */
export function useUnsavedChangesWarning(dirty: boolean) {
  useEffect(() => {
    if (!dirty) return;
    function handleBeforeUnload(event: BeforeUnloadEvent) {
      event.preventDefault();
      // Required by some browsers to actually show the prompt.
      event.returnValue = "";
    }
    window.addEventListener("beforeunload", handleBeforeUnload);
    return () => window.removeEventListener("beforeunload", handleBeforeUnload);
  }, [dirty]);
}
