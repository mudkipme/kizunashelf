import { Trans } from "@lingui/react/macro";
import { Link } from "react-router-dom";

import { AppFrame } from "@/components/layout/app-frame";
import { Button } from "@/components/ui/button";
import { Placeholder } from "@/components/ui/placeholder";

// Catch-all for unknown/stale URLs. Without it an unmatched route renders an
// empty `<Routes>` (a blank frame); this degrades it to a recoverable screen.
export function NotFoundPage() {
  return (
    <AppFrame>
      <div className="mx-auto flex w-full max-w-2xl flex-col gap-4 p-4">
        <Placeholder className="flex flex-col items-center gap-3 py-16">
          <p className="text-base font-semibold text-foreground">
            <Trans>Page not found</Trans>
          </p>
          <p>
            <Trans>This page doesn’t exist or may have moved.</Trans>
          </p>
          <Button asChild>
            <Link to="/">
              <Trans>Back to home</Trans>
            </Link>
          </Button>
        </Placeholder>
      </div>
    </AppFrame>
  );
}
