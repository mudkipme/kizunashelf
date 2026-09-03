import * as React from "react";

import { cn } from "@/lib/utils";

function Input({ className, type, ...props }: React.ComponentProps<"input">) {
  return (
    <input
      type={type}
      className={cn(
        "flex h-(--control-height) w-full rounded-md border border-input bg-background px-2.5 py-1 text-sm shadow-xs ring-offset-background transition-colors outline-none placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50 pointer-coarse:text-base",
        className,
      )}
      {...props}
    />
  );
}

export { Input };
