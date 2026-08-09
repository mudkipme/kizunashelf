import { useEffect, useState } from "react";

import { isMacDesktopRuntime } from "@/lib/desktop";

/**
 * Whether the app header must reserve room for the macOS traffic lights. The
 * desktop shell uses `titleBarStyle: Overlay`, which draws the native window
 * buttons on top of the web content; macOS hides them in fullscreen, so the
 * inset tracks fullscreen transitions. Always false on the web and on
 * Windows/Linux, which keep their native title bars.
 */
export function useMacTitlebarInset(): boolean {
  const [inset, setInset] = useState(() => isMacDesktopRuntime());

  useEffect(() => {
    if (!isMacDesktopRuntime()) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void (async () => {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      const appWindow = getCurrentWindow();
      const sync = async () => {
        // On failure assume windowed, keeping the inset (buttons visible).
        const fullscreen = await appWindow.isFullscreen().catch(() => false);
        if (!cancelled) setInset(!fullscreen);
      };
      await sync();
      // Fullscreen transitions surface as resizes; there is no dedicated event.
      const stop = await appWindow.onResized(() => void sync());
      if (cancelled) stop();
      else unlisten = stop;
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  return inset;
}
