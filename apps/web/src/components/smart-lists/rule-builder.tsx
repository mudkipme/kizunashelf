//! The criteria editor: a match mode over a list of rules and nested groups.
//!
//! Split across sibling modules — `rule-field-meta` (what can be asked of which
//! field), `rule-editor-state` (rule <-> editor translation), `rule-row`,
//! `rule-value-input` and `rule-subgroup` — leaving this file as the shell that
//! composes them.

import { Trans } from "@lingui/react/macro";
import { CopyPlusIcon, PlusIcon } from "lucide-react";

import { defaultRuleFor } from "@/components/smart-lists/rule-editor-state";
import type { RuleFieldMeta } from "@/components/smart-lists/rule-field-meta";
import { RuleRow } from "@/components/smart-lists/rule-row";
import { ConjunctionSelect, SubgroupBox } from "@/components/smart-lists/rule-subgroup";
import { Button } from "@/components/ui/button";
import type { SmartFilterGroup, SmartFilterRule, SmartFilterSubgroup } from "@/types/api";

export function RuleBuilder({
  fieldMetas,
  value,
  onChange,
  disabled = false,
}: {
  fieldMetas: RuleFieldMeta[];
  value: SmartFilterGroup;
  onChange: (next: SmartFilterGroup) => void;
  disabled?: boolean;
}) {
  const rules = value.rules ?? [];
  const groups = value.groups ?? [];

  const replaceRule = (index: number, rule: SmartFilterRule | null) => {
    const next = rules.flatMap((existing, i) => (i === index ? (rule ? [rule] : []) : [existing]));
    onChange({ ...value, rules: next });
  };
  const replaceGroup = (index: number, group: SmartFilterSubgroup | null) => {
    const next = groups.flatMap((existing, i) =>
      i === index ? (group ? [group] : []) : [existing],
    );
    onChange({ ...value, groups: next });
  };

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <ConjunctionSelect
          value={value.conjunction}
          disabled={disabled}
          onChange={(conjunction) => onChange({ ...value, conjunction })}
        />
        <span className="text-xs text-muted-foreground">
          <Trans>of the following match:</Trans>
        </span>
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
      {groups.map((group, index) => (
        <SubgroupBox
          key={index}
          group={group}
          fieldMetas={fieldMetas}
          disabled={disabled}
          onChange={(next) => replaceGroup(index, next)}
        />
      ))}
      <div className="flex items-center gap-2">
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={disabled || fieldMetas.length === 0}
          onClick={() => onChange({ ...value, rules: [...rules, defaultRuleFor(fieldMetas[0])] })}
        >
          <PlusIcon data-icon="inline-start" />
          <Trans>Add rule</Trans>
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={disabled || fieldMetas.length === 0}
          onClick={() =>
            onChange({
              ...value,
              groups: [...groups, { conjunction: "any", rules: [defaultRuleFor(fieldMetas[0])] }],
            })
          }
        >
          <CopyPlusIcon data-icon="inline-start" />
          <Trans>Add group</Trans>
        </Button>
      </div>
    </div>
  );
}
