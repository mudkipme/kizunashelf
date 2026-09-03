import { cva, type VariantProps } from "class-variance-authority";
import * as React from "react";

import { cn } from "@/lib/utils";

const alertVariants = cva("rounded-md border p-3 text-sm", {
  variants: {
    variant: {
      destructive: "border-destructive/40 bg-destructive/10 text-destructive",
    },
  },
  defaultVariants: {
    variant: "destructive",
  },
});

/**
 * Inline status banner. Replaces the repeated destructive box literal. Pass a
 * `className` to add layout (e.g. a row with an action button).
 */
export function Alert({
  className,
  variant,
  ...props
}: React.ComponentProps<"div"> & VariantProps<typeof alertVariants>) {
  return <div className={cn(alertVariants({ variant }), className)} {...props} />;
}

export { alertVariants };
