import * as React from "react";

import { cn } from "@/lib/utils";

/**
 * Centered, bordered box for non-content states — loading, empty, and
 * not-found. Replaces the inline `rounded-md border p-8 …` div repeated across
 * pages so the spacing and muted styling stay consistent.
 */
export function Placeholder({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      className={cn("rounded-md border p-8 text-center text-sm text-muted-foreground", className)}
      {...props}
    />
  );
}
