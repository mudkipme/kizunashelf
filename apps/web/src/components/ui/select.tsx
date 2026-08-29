import * as React from "react";

import { cn } from "@/lib/utils";

function Select({ className, ...props }: React.ComponentProps<"select">) {
  return (
    <select
      className={cn(
        "border-input bg-background ring-offset-background focus-visible:ring-ring h-(--control-height) rounded-md border px-2 text-sm shadow-xs outline-none transition-colors focus-visible:ring-2 disabled:cursor-not-allowed disabled:opacity-50 pointer-coarse:text-base",
        className,
      )}
      {...props}
    />
  );
}

export { Select };
