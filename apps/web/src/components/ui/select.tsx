import * as React from "react";

import { cn } from "@/lib/utils";

function Select({ className, ...props }: React.ComponentProps<"select">) {
  return (
    <select
      className={cn(
        "h-(--control-height) rounded-md border border-input bg-background px-2 text-sm shadow-xs ring-offset-background transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50 pointer-coarse:text-base",
        className,
      )}
      {...props}
    />
  );
}

export { Select };
