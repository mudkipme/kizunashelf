//! One rule of a criteria group: the field picker, the operator picker, and the
//! value editor those two choose between.

import { useMemo } from "react";
import { useLingui } from "@lingui/react/macro";
import { XIcon } from "lucide-react";

import {
  defaultRuleFor,
  editorFromRule,
  ruleFromEditor,
  type EditorState,
} from "@/components/smart-lists/rule-editor-state";
import { opWord, opsByKind, type RuleFieldMeta } from "@/components/smart-lists/rule-field-meta";
import { formatRule } from "@/components/smart-lists/rule-format";
import { RuleValueInput } from "@/components/smart-lists/rule-value-input";
import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";
import type { SmartFilterRule } from "@/types/api";

export function RuleRow({
  rule,
  fieldMetas,
  disabled,
  onChange,
}: {
  rule: SmartFilterRule;
  fieldMetas: RuleFieldMeta[];
  disabled: boolean;
  onChange: (next: SmartFilterRule | null) => void;
}) {
  const { t } = useLingui();
  const editor = useMemo(() => editorFromRule(rule, fieldMetas, t), [rule, fieldMetas, t]);

  if (!editor) {
    // Outside the builder's vocabulary (hand-written syntax) — shown, kept on
    // save, deletable, but not editable here.
    return (
      <div className="flex items-center gap-2 rounded-md border bg-muted/40 px-2 py-1.5">
        <code className="min-w-0 flex-1 truncate text-xs text-muted-foreground">
          {formatRule(rule, t)}
        </code>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={disabled}
          aria-label={t`Remove rule`}
          onClick={() => onChange(null)}
        >
          <XIcon />
        </Button>
      </div>
    );
  }

  const { meta, op, inputs } = editor;
  const emit = (nextMeta: RuleFieldMeta, nextOp: string, nextInputs: EditorState["inputs"]) =>
    onChange(ruleFromEditor(nextMeta, nextOp, nextInputs));

  return (
    <div className="flex flex-wrap items-center gap-2">
      <Select
        value={meta.key}
        disabled={disabled}
        aria-label={t`Field`}
        className="w-36 min-w-0"
        onChange={(event) => {
          const next = fieldMetas.find((candidate) => candidate.key === event.target.value);
          if (next) onChange(defaultRuleFor(next));
        }}
      >
        {/* A field the schema doesn't declare — hand-written, or the synthetic
            "any link" scope — still shows itself. Picking a real field replaces
            the rule, so the switch is deliberately one-way. */}
        {fieldMetas.some((candidate) => candidate.key === meta.key) ? null : (
          <option value={meta.key}>{meta.label}</option>
        )}
        {fieldMetas.map((candidate) => (
          <option key={candidate.key} value={candidate.key}>
            {candidate.label}
          </option>
        ))}
      </Select>
      <Select
        value={op}
        disabled={disabled}
        aria-label={t`Operator`}
        className="w-fit min-w-0"
        onChange={(event) => emit(meta, event.target.value, inputs)}
      >
        {opsByKind[meta.kind].map((candidate) => (
          <option key={candidate} value={candidate}>
            {t(opWord[candidate])}
          </option>
        ))}
      </Select>
      <RuleValueInput
        meta={meta}
        op={op}
        inputs={inputs}
        disabled={disabled}
        onChange={(nextInputs) => emit(meta, op, nextInputs)}
      />
      <Button
        type="button"
        variant="ghost"
        size="sm"
        disabled={disabled}
        aria-label={t`Remove rule`}
        onClick={() => onChange(null)}
      >
        <XIcon />
      </Button>
    </div>
  );
}
