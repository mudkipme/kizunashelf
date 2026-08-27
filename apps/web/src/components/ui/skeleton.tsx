import * as React from "react";

import { cn } from "@/lib/utils";

/**
 * A pulsing stand-in for content that has not arrived yet. Purely decorative —
 * the caller sizes it to the box the real thing will occupy and announces the
 * loading state itself (`role="status"` plus `sr-only` text), so the skeleton
 * stays out of the accessibility tree instead of reading as empty elements.
 *
 * The fill is a tint of `foreground` rather than a surface token because
 * skeletons appear on both surfaces the app has, and `muted` is the same value
 * as `chrome` in the light theme — a `bg-muted` skeleton is invisible in the
 * sidebar. A translucent foreground reads against either one.
 */
export function Skeleton({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      aria-hidden="true"
      className={cn("animate-pulse rounded-md bg-foreground/10", className)}
      {...props}
    />
  );
}
