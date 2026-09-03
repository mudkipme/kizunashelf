import type { ComponentProps } from "react";

import { cn } from "@/lib/utils";

// No `pointer-coarse:text-base` guard here, unlike the single-line fields:
// `--text-prose` is already 16px on touch, so a focused textarea clears iOS
// Safari's zoom floor on its own — and adding the variant anyway would beat a
// caller's own size (`text-code`, on the notes and raw-config editors), which
// tailwind-merge cannot drop because the modifier differs.
export function Textarea({ className, ...props }: ComponentProps<"textarea">) {
  return (
    <textarea
      className={cn(
        "min-h-24 w-full rounded-md border border-input bg-background px-2.5 py-1.5 text-prose shadow-xs transition-[color,box-shadow] outline-none placeholder:text-muted-foreground disabled:cursor-not-allowed disabled:opacity-50",
        "focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50",
        "aria-invalid:border-destructive aria-invalid:ring-destructive/20",
        className,
      )}
      {...props}
    />
  );
}
