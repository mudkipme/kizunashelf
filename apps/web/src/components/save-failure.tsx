import { Trans } from "@lingui/react/macro";

import { errorMessage, isConflictError } from "@/api/client";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";

/** A failed save stays next to its draft until retry or explicit recovery. */
export function SaveFailure({
  error,
  recover,
  recovering = false,
  revisionConflict = isConflictError(error),
}: {
  error: unknown;
  recover?: () => void;
  recovering?: boolean;
  revisionConflict?: boolean;
}) {
  if (!error) return null;
  return (
    <Alert role="alert" className="flex flex-col items-start gap-2">
      <p>
        {revisionConflict ? (
          <Trans>
            This changed elsewhere. Your edits are kept. Load the latest version before trying
            again.
          </Trans>
        ) : (
          errorMessage(error)
        )}
      </p>
      {recover ? (
        <Button variant="outline" size="sm" disabled={recovering} onClick={recover}>
          <Trans>Reload</Trans>
        </Button>
      ) : null}
    </Alert>
  );
}
