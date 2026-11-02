import { useEffect } from "react";

import { isDesktopRuntime, setUnsavedChanges } from "@/lib/desktop";

/**
 * Warn the user before they close/reload the tab while `dirty` is true.
 *
 * Desktop also shares this state with the vault manager so switching vaults
 * from another window can ask before discarding edits. Route changes are
 * handled by the caller's navigation blocker.
 */
export function useUnsavedChangesWarning(dirty: boolean) {
  useEffect(() => {
    if (!isDesktopRuntime()) return;
    void setUnsavedChanges(dirty).catch(console.error);
    return () => {
      void setUnsavedChanges(false).catch(console.error);
    };
  }, [dirty]);

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
