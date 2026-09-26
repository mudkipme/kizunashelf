import type { EntityEditDraft, EntityEditReviewResponse } from "@kizunashelf/api-contract";
import { Trans, useLingui } from "@lingui/react/macro";
import { useId, useState } from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";

export type EditReviewChoices = {
  fields: Record<string, "mine" | "latest">;
  body?: "mine" | "latest";
  name?: "mine" | "latest";
};

/** Apply explicit choices to the core's merge; never derive conflict semantics here. */
export function acceptEditReview(
  review: EntityEditReviewResponse,
  choices: EditReviewChoices,
): EntityEditDraft {
  const draft = { ...review.merged, frontmatter: { ...review.merged.frontmatter } };
  for (const field of review.conflictFields) {
    if (choices.fields[field] !== "mine") continue;
    if (Object.hasOwn(review.local.frontmatter, field))
      draft.frontmatter[field] = review.local.frontmatter[field];
    else delete draft.frontmatter[field];
  }
  if (review.bodyConflict && choices.body === "mine") draft.body = review.local.body;
  if (review.nameConflict && choices.name === "mine") draft.basename = review.local.basename;
  return draft;
}

export function EditConflictReview({
  review,
  onAccept,
  onCancel,
  label,
}: {
  review: EntityEditReviewResponse;
  onAccept: (draft: EntityEditDraft) => void;
  onCancel: () => void;
  label: (field: string) => string;
}) {
  const { t } = useLingui();
  const [choices, setChoices] = useState<EditReviewChoices>({ fields: {} });
  const complete =
    review.conflictFields.every((field) => choices.fields[field]) &&
    (!review.bodyConflict || choices.body) &&
    (!review.nameConflict || choices.name);
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onCancel();
      }}
    >
      <DialogContent className="sm:max-w-xl" aria-describedby={undefined}>
        <DialogHeader>
          <DialogTitle>
            <Trans>Review changes</Trans>
          </DialogTitle>
        </DialogHeader>
        <p className="text-sm text-muted-foreground">
          <Trans>
            Non-conflicting changes are combined. Choose which value to keep where both versions
            changed. Nothing is saved until you press Save.
          </Trans>
        </p>
        <div className="flex max-h-[55vh] flex-col gap-4 overflow-auto">
          {review.conflictFields.map((field) => (
            <ConflictChoice
              key={field}
              label={label(field)}
              mine={review.local.frontmatter[field]}
              latest={review.entity.frontmatter[field]}
              choice={choices.fields[field]}
              onChange={(choice) =>
                setChoices({ ...choices, fields: { ...choices.fields, [field]: choice } })
              }
            />
          ))}
          {review.bodyConflict ? (
            <ConflictChoice
              label={t`Notes`}
              mine={review.local.body}
              latest={review.entity.body}
              choice={choices.body}
              onChange={(body) => setChoices({ ...choices, body })}
            />
          ) : null}
          {review.nameConflict ? (
            <ConflictChoice
              label={t`Filename`}
              mine={review.local.basename}
              latest={review.entity.basename}
              choice={choices.name}
              onChange={(name) => setChoices({ ...choices, name })}
            />
          ) : null}
          {!review.conflictFields.length && !review.bodyConflict && !review.nameConflict ? (
            <p className="text-sm">
              <Trans>Your edits can be combined with the latest version.</Trans>
            </p>
          ) : null}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onCancel}>
            <Trans>Keep editing</Trans>
          </Button>
          <Button disabled={!complete} onClick={() => onAccept(acceptEditReview(review, choices))}>
            <Trans>Use reviewed draft</Trans>
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function ConflictChoice({
  label,
  mine,
  latest,
  choice,
  onChange,
}: {
  label: string;
  mine: unknown;
  latest: unknown;
  choice?: "mine" | "latest";
  onChange: (choice: "mine" | "latest") => void;
}) {
  const name = useId();
  return (
    <fieldset className="min-w-0">
      <legend className="mb-2 text-sm font-medium">{label}</legend>
      <div className="grid grid-cols-2 gap-2">
        {(["mine", "latest"] as const).map((side) => (
          <label key={side} className="min-w-0 rounded-md border p-2 text-sm">
            <span className="flex items-center gap-2">
              <input
                type="radio"
                name={name}
                checked={choice === side}
                onChange={() => onChange(side)}
              />
              {side === "mine" ? <Trans>My edits</Trans> : <Trans>Latest version</Trans>}
            </span>
            <pre className="mt-2 max-h-40 overflow-auto text-xs break-words whitespace-pre-wrap">
              {(side === "mine" ? mine : latest) === undefined ? (
                <Trans>Removed</Trans>
              ) : typeof (side === "mine" ? mine : latest) === "string" ? (
                String(side === "mine" ? mine : latest)
              ) : (
                JSON.stringify(side === "mine" ? mine : latest, null, 2)
              )}
            </pre>
          </label>
        ))}
      </div>
    </fieldset>
  );
}
