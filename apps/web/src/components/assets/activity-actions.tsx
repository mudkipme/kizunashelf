import { Trans, useLingui } from "@lingui/react/macro";
import { ChevronDownIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
} from "@/components/ui/dropdown-menu";
import type { LogKind } from "@/types/api";

/** Entry points into one activity sheet, with the desired action preselected. */
export function ActivityActions({
  kinds,
  onSelect,
}: {
  kinds: LogKind[];
  onSelect: (kind: LogKind) => void;
}) {
  const { t } = useLingui();
  const [primary, ...secondary] = kinds;
  if (!primary) return null;
  return (
    <div className="flex items-center gap-1">
      <Button variant="outline" size="sm" onClick={() => onSelect(primary)}>
        {primary === "started" ? (
          <Trans>Start</Trans>
        ) : primary === "completed" ? (
          <Trans>Finish</Trans>
        ) : (
          <Trans>Log activity</Trans>
        )}
      </Button>
      {secondary.length > 0 ? (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="outline" size="icon-sm" aria-label={t`Activity actions`}>
              <ChevronDownIcon />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            {secondary.map((kind) => (
              <DropdownMenuItem key={kind} onSelect={() => onSelect(kind)}>
                {kind === "started" ? (
                  <Trans>Start</Trans>
                ) : kind === "completed" ? (
                  <Trans>Finish</Trans>
                ) : (
                  <Trans>Log activity</Trans>
                )}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      ) : null}
    </div>
  );
}
