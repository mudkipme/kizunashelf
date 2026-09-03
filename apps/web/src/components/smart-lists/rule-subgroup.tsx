//! A nested criteria group — its own match mode over its own rules — plus the
//! match-mode picker it shares with the top-level builder.

import { Trans, useLingui } from "@lingui/react/macro";
import { PlusIcon, XIcon } from "lucide-react";

import { defaultRuleFor } from "@/components/smart-lists/rule-editor-state";
import type { RuleFieldMeta } from "@/components/smart-lists/rule-field-meta";
import { RuleRow } from "@/components/smart-lists/rule-row";
import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";
import type { SmartFilterConjunction, SmartFilterRule, SmartFilterSubgroup } from "@/types/api";

export function SubgroupBox({
  group,
  fieldMetas,
  disabled,
  onChange,
}: {
  group: SmartFilterSubgroup;
  fieldMetas: RuleFieldMeta[];
  disabled: boolean;
  onChange: (next: SmartFilterSubgroup | null) => void;
}) {
  const { t } = useLingui();
  const rules = group.rules ?? [];
  const replaceRule = (index: number, rule: SmartFilterRule | null) => {
    const next = rules.flatMap((existing, i) => (i === index ? (rule ? [rule] : []) : [existing]));
    // A group emptied of its last rule disappears rather than lingering.
    if (next.length === 0) onChange(null);
    else onChange({ ...group, rules: next });
  };
  return (
    <div className="flex flex-col gap-2 rounded-md border border-dashed p-2">
      <div className="flex items-center gap-2">
        <ConjunctionSelect
          value={group.conjunction}
          disabled={disabled}
          onChange={(conjunction) => onChange({ ...group, conjunction })}
        />
        <span className="mr-auto text-xs text-muted-foreground">
          <Trans>of the following match:</Trans>
        </span>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={disabled}
          aria-label={t`Remove group`}
          onClick={() => onChange(null)}
        >
          <XIcon />
        </Button>
      </div>
      {rules.map((rule, index) => (
        <RuleRow
          key={index}
          rule={rule}
          fieldMetas={fieldMetas}
          disabled={disabled}
          onChange={(next) => replaceRule(index, next)}
        />
      ))}
      <Button
        type="button"
        variant="outline"
        size="sm"
        className="self-start"
        disabled={disabled || fieldMetas.length === 0}
        onClick={() => onChange({ ...group, rules: [...rules, defaultRuleFor(fieldMetas[0])] })}
      >
        <PlusIcon data-icon="inline-start" />
        <Trans>Add rule</Trans>
      </Button>
    </div>
  );
}

export function ConjunctionSelect({
  value,
  disabled,
  onChange,
}: {
  value: SmartFilterConjunction;
  disabled: boolean;
  onChange: (value: SmartFilterConjunction) => void;
}) {
  const { t } = useLingui();
  return (
    <Select
      value={value}
      disabled={disabled}
      aria-label={t`Match mode`}
      className="w-fit"
      onChange={(event) => onChange(event.target.value as SmartFilterConjunction)}
    >
      <option value="all">{t`All`}</option>
      <option value="any">{t`Any`}</option>
      <option value="none">{t`None`}</option>
    </Select>
  );
}
