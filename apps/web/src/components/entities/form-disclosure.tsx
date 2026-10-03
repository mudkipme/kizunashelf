import { ChevronRightIcon } from "lucide-react";
import { Collapsible } from "radix-ui";
import type { ReactNode } from "react";

import { Button } from "@/components/ui/button";

/** Optional form controls remain in the draft when their panel is closed. */
export function FormDisclosure({
  title,
  children,
  open,
  onOpenChange,
}: {
  title: string;
  children: ReactNode;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
}) {
  return (
    <Collapsible.Root open={open} onOpenChange={onOpenChange}>
      <Collapsible.Trigger asChild>
        <Button type="button" variant="ghost" size="sm" className="group -ml-2">
          <ChevronRightIcon
            data-icon="inline-start"
            className="transition-transform group-data-[state=open]:rotate-90"
          />
          {title}
        </Button>
      </Collapsible.Trigger>
      <Collapsible.Content forceMount className="pt-3 data-[state=closed]:hidden">
        {children}
      </Collapsible.Content>
    </Collapsible.Root>
  );
}
