//! The words the import wizard puts on provider buckets and canonical statuses.
//!
//! A provider names its own buckets (Bangumi's numeric collection types, for
//! instance); an unrecognized one falls through to its raw id rather than being
//! hidden, so a provider that adds a bucket still imports.

import type { I18n, MessageDescriptor } from "@lingui/core";
import { msg } from "@lingui/core/macro";

import type { ImportCanonicalStatus, ImportJobStatus, ImportPlanItem } from "@/types/api";

export function isActive(status?: ImportJobStatus) {
  return status === "queued" || status === "fetching" || status === "committing";
}

// Bangumi buckets are numeric `subject_type` codes; give them readable labels.
// Every other provider's bucket is already a word (anime, movie, book…).
const BANGUMI_BUCKETS: Record<string, MessageDescriptor> = {
  "1": msg`Book`,
  "2": msg`Anime`,
  "3": msg`Music`,
  "4": msg`Game`,
  "6": msg`Real`,
};
export function bucketLabel(i18n: I18n, provider: string, bucket: string): string {
  if (provider === "bangumi" && BANGUMI_BUCKETS[bucket]) return i18n._(BANGUMI_BUCKETS[bucket]);
  return bucket || "—";
}

const STATUS_LABELS: Record<ImportCanonicalStatus, MessageDescriptor> = {
  planning: msg`Planning`,
  ongoing: msg`Ongoing`,
  paused: msg`Paused`,
  completed: msg`Completed`,
  dropped: msg`Dropped`,
};
export function statusLabel(i18n: I18n, status: ImportCanonicalStatus): string {
  return STATUS_LABELS[status] ? i18n._(STATUS_LABELS[status]) : status;
}

export function reviewReasonLabel(reason: ImportPlanItem["reviewReason"]): MessageDescriptor {
  switch (reason) {
    case "noSupportedId":
      return msg`No matched provider`;
    case "noTypeMatch":
      return msg`No matching type`;
    case "providerUnavailable":
      return msg`Provider unavailable`;
    default:
      return msg`Needs review`;
  }
}
