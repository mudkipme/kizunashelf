import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { StarIcon } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

import { errorMessage, isConflictError } from "@/api/client";
import { saveRating } from "@/api/entities";
import { useInvalidateEntityData } from "@/api/invalidate-entity-data";
import { entityQuery } from "@/api/queries";
import { SaveFailure } from "@/components/save-failure";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { RatingScore } from "@/components/ui/rating-score";
import { useCapabilities } from "@/lib/capabilities";
import type { EntitySummary } from "@/types/api";

export function EntityRatings({
  entity,
  openField,
  onOpenFieldChange,
}: {
  entity: EntitySummary;
  openField?: string | null;
  onOpenFieldChange?: (field: string | null) => void;
}) {
  const capabilities = useCapabilities();
  const [localField, setLocalField] = useState<string | null>(null);
  const selected = openField === undefined ? localField : openField;
  const setSelected = onOpenFieldChange ?? setLocalField;
  return (
    <div className="flex flex-wrap items-center gap-1">
      {(entity.ratings ?? []).map((rating) => (
        <RatingPicker
          key={rating.field}
          entity={entity}
          rating={rating}
          writable={capabilities.contentWritable && !capabilities.isPending}
          open={selected === rating.field}
          onOpenChange={(open) => setSelected(open ? rating.field : null)}
        />
      ))}
    </div>
  );
}

type Rating = NonNullable<EntitySummary["ratings"]>[number];

function RatingPicker({
  entity,
  rating,
  writable,
  open,
  onOpenChange,
}: {
  entity: EntitySummary;
  rating: Rating;
  writable: boolean;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useLingui();
  const multiple = (entity.ratings?.length ?? 0) > 1;
  const label = (
    <>
      {multiple ? <span>{rating.label}</span> : null}
      {rating.value != null ? (
        <RatingScore value={rating.value} max={rating.max} />
      ) : (
        <>
          <StarIcon className="size-3.5" />
          <Trans>Rate</Trans>
        </>
      )}
    </>
  );
  if (!writable)
    return rating.value != null ? (
      <span className="inline-flex items-center gap-1 px-2 text-sm">{label}</span>
    ) : null;
  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger asChild>
        <Button variant="ghost" size="sm" aria-label={t`Rate ${rating.label}`}>
          {label}
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-80" aria-label={rating.label}>
        {open ? (
          <RatingEditor
            entityId={entity.id}
            field={rating.field}
            onSaved={() => onOpenChange(false)}
          />
        ) : null}
      </PopoverContent>
    </Popover>
  );
}

function RatingEditor({
  entityId,
  field,
  onSaved,
}: {
  entityId: string;
  field: string;
  onSaved: () => void;
}) {
  const { t } = useLingui();
  const queryClient = useQueryClient();
  const invalidate = useInvalidateEntityData();
  const detail = useQuery({ ...entityQuery(entityId), staleTime: 0, refetchOnWindowFocus: false });
  const [draft, setDraft] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<unknown>();
  const entity = detail.data?.entity;
  const rating = entity?.ratings?.find((item) => item.field === field);
  const exact = draft ?? (rating?.value != null ? String(rating.value) : "");
  const number = exact.trim() ? Number(exact) : NaN;
  const valid =
    Number.isFinite(number) && (rating?.max == null || (number >= 0 && number <= rating.max));
  const buttons = rating?.max != null && Number.isInteger(rating.max) && rating.max <= 10;

  async function save(value: number | null) {
    if (!entity || !rating || saving) return;
    if (value !== null) setDraft(String(value));
    setSaving(true);
    setError(undefined);
    try {
      const result = await saveRating(entityId, {
        revision: entity.revision,
        field,
        max: rating.max,
        value,
      });
      queryClient.setQueryData(entityQuery(entityId).queryKey, (current) =>
        current ? { ...current, entity: result.entity } : current,
      );
      void invalidate();
      onSaved();
      // Keep the revision produced by this save. Undo must never overwrite a
      // later edit, even if background queries have since refreshed the entity.
      toast.success(value === null ? t`Rating cleared` : t`Rating saved`, {
        action: {
          label: t`Undo`,
          onClick: () => {
            void saveRating(entityId, {
              revision: result.entity.revision,
              field,
              max: rating.max,
              restore: result.previous,
            })
              .then(() => {
                void invalidate();
                toast.success(t`Rating restored`);
              })
              .catch((failure: unknown) => {
                toast.error(errorMessage(failure));
                void invalidate();
              });
          },
        },
      });
    } catch (failure) {
      setError(failure);
    } finally {
      setSaving(false);
    }
  }

  if (!entity || !rating)
    return (
      <div className="text-sm">
        {detail.error ? (
          <>
            <p role="alert">{errorMessage(detail.error)}</p>
            <Button variant="ghost" onClick={() => void detail.refetch()}>
              <Trans>Retry</Trans>
            </Button>
          </>
        ) : detail.isPending ? (
          <Trans>Loading…</Trans>
        ) : (
          <Trans>This rating field is no longer available.</Trans>
        )}
      </div>
    );
  return (
    <div className="flex flex-col gap-3" aria-busy={saving || detail.isFetching}>
      <div className="flex items-center justify-between gap-2">
        <h2 className="text-sm font-medium">{rating.label}</h2>
        {rating.value != null ? (
          <RatingScore value={rating.value} max={rating.max} />
        ) : (
          <span className="text-sm text-muted-foreground">
            <Trans>Unrated</Trans>
          </span>
        )}
      </div>
      <fieldset
        disabled={saving || detail.isFetching || isConflictError(error)}
        className="flex min-w-0 flex-col gap-3"
      >
        {buttons ? (
          <div className="grid grid-cols-6 gap-1">
            {Array.from({ length: rating.max! + 1 }, (_, score) => (
              <Button
                key={score}
                className="h-11 px-0"
                variant={rating.value === score ? "default" : "outline"}
                aria-pressed={rating.value === score}
                onClick={() => void save(score)}
              >
                {score}
              </Button>
            ))}
          </div>
        ) : null}
        <details open={!buttons || undefined}>
          {buttons ? (
            <summary className="cursor-pointer text-sm text-muted-foreground">
              <Trans>Exact score</Trans>
            </summary>
          ) : null}
          <form
            className="mt-2 flex items-center gap-2"
            onSubmit={(event) => {
              event.preventDefault();
              if (valid) void save(number);
            }}
          >
            <Input
              aria-label={t`Exact score`}
              type="number"
              step="any"
              min={rating.max == null ? undefined : 0}
              max={rating.max ?? undefined}
              value={exact}
              onChange={(event) => setDraft(event.target.value)}
            />
            <Button type="submit" disabled={!valid}>
              <Trans>Save</Trans>
            </Button>
          </form>
        </details>
        <Button
          variant="ghost"
          className="self-start"
          disabled={!Object.hasOwn(entity.frontmatter, field)}
          onClick={() => void save(null)}
        >
          <Trans>Clear rating</Trans>
        </Button>
      </fieldset>
      <SaveFailure
        error={error}
        recovering={detail.isFetching}
        recover={
          isConflictError(error)
            ? async () => {
                const refreshed = await detail.refetch();
                if (!refreshed.error) setError(undefined);
              }
            : undefined
        }
      />
    </div>
  );
}
