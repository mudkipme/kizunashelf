import type { ReactNode } from "react";
import { Trans, useLingui } from "@lingui/react/macro";

import { formatRule } from "@/components/smart-lists/rule-format";
import { Badge } from "@/components/ui/badge";
import type { SmartFilterGroup, SmartFilterRule } from "@/types/api";

/// Read-only rendering of a criteria group. Rules render as compact chips;
/// nested groups as bordered clusters with their own conjunction. Shared by the
/// smart-list page and the library browser, which show the same thing — one is
/// a saved list, the other an unsaved one.
export function CriteriaSummary({
  group,
  empty,
}: {
  group: SmartFilterGroup;
  /// What to show when nothing is constrained; the caller words it for its own
  /// context (a scoped list vs. the whole library).
  empty?: ReactNode;
}) {
  const rules = group.rules ?? [];
  const subgroups = group.groups ?? [];
  if (rules.length === 0 && subgroups.length === 0) {
    return empty ? <p className="text-xs text-muted-foreground">{empty}</p> : null;
  }
  return (
    <div className="flex flex-wrap items-center gap-1.5 text-xs">
      <ConjunctionLabel conjunction={group.conjunction} />
      {rules.map((rule, index) => (
        <RuleChip key={index} rule={rule} />
      ))}
      {subgroups.map((subgroup, index) => (
        <span
          key={index}
          className="flex flex-wrap items-center gap-1.5 rounded-md border border-dashed px-1.5 py-1"
        >
          <ConjunctionLabel conjunction={subgroup.conjunction} />
          {(subgroup.rules ?? []).map((rule, ruleIndex) => (
            <RuleChip key={ruleIndex} rule={rule} />
          ))}
        </span>
      ))}
    </div>
  );
}

function ConjunctionLabel({ conjunction }: { conjunction: SmartFilterGroup["conjunction"] }) {
  return (
    <span className="font-medium text-muted-foreground">
      {conjunction === "any" ? (
        <Trans comment="Prefix before a group of filter-criteria chips: at least one must match">
          any of
        </Trans>
      ) : conjunction === "none" ? (
        <Trans comment="Prefix before a group of filter-criteria chips: none may match">
          none of
        </Trans>
      ) : (
        <Trans comment="Prefix before a group of filter-criteria chips: all must match">
          all of
        </Trans>
      )}
    </span>
  );
}

function RuleChip({ rule }: { rule: SmartFilterRule }) {
  const { t } = useLingui();
  const label = formatRule(rule, t);
  const ignored = rule.kind === "unsupported";
  return (
    <Badge variant="outline" className={ignored ? "text-muted-foreground line-through" : undefined}>
      {label}
    </Badge>
  );
}
