import type { CSSProperties } from "react";
import { Toaster as Sonner, type ToasterProps } from "sonner";

import { useThemeStore } from "@/lib/theme";

/**
 * App toaster. Follows the manual `.dark` theme (see `lib/theme.ts`) by handing
 * sonner the store's mode directly, and maps its surface variables onto the
 * project's `--color-*` tokens so the default toast matches cards/popovers.
 * `richColors` supplies the semantic green/red for success and error toasts,
 * which the theme has no dedicated token for.
 */
export function Toaster(props: ToasterProps) {
  const mode = useThemeStore((state) => state.mode);
  return (
    <Sonner
      theme={mode}
      position="bottom-right"
      richColors
      closeButton
      style={
        {
          "--normal-bg": "var(--color-popover)",
          "--normal-text": "var(--color-popover-foreground)",
          "--normal-border": "var(--color-border)",
          "--border-radius": "var(--radius-md)",
        } as CSSProperties
      }
      {...props}
    />
  );
}
