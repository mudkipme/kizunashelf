import { useMemo, useState } from "react";
import { plural } from "@lingui/core/macro";
import { Plural, Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { CheckIcon } from "lucide-react";
import { toast } from "sonner";

import { errorMessage } from "@/api/client";
import { languagesQuery, typePresetsQuery } from "@/api/queries";
import { resolveTypePresets } from "@/api/settings";
import { Alert } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Select } from "@/components/ui/select";
import { useLanguagePreference } from "@/lib/language";
import type {
  EntityTypeConfig,
  TypePresetBackfill,
  TypePresetSummary,
  UserLanguage,
} from "@/types/api";

/**
 * The languages the preset *seed text* (labels, status values, folder names) is
 * written in — mirrors `SeedLocale` in the core's `presets.rs`. Any other
 * choice still gets its titles in that language, just with English labels; the
 * picker shows an explicit hint for those.
 */
const SEED_TEXT_LANGUAGES = new Set(["en", "ja", "zh-Hans", "zh-Hant"]);

/**
 * The "add built-in type" picker. A category-grouped, multi-select gallery over
 * `GET /api/type-presets`; on confirm it resolves the picks against the editor's
 * current types (relations wired/stripped, collisions suffixed, back-fills
 * proposed) and returns the merged `types` array. Presets already present in the
 * schema (matched by id) show as "Added" and can't be re-added.
 *
 * Home sections are intentionally out of scope here — the settings editor owns
 * Home separately, and onboarding (which builds the whole vault) wires them.
 */
export function PresetPickerDialog({
  currentTypes,
  onClose,
  onApply,
}: {
  currentTypes: EntityTypeConfig[];
  onClose: () => void;
  onApply: (nextTypes: EntityTypeConfig[]) => void;
}) {
  const { t } = useLingui();
  // The app's language preference localizes the picker cards and is the
  // natural default for the single language choice below (which drives both
  // the seeded labels and the title language of the new types).
  const preference = useLanguagePreference();
  const presets = useQuery(typePresetsQuery(preference));
  const userLanguages = useQuery(languagesQuery()).data?.userLanguages ?? [];
  const [selected, setSelected] = useState<Set<string>>(() => new Set());
  const [language, setLanguage] = useState(preference);
  const [resolving, setResolving] = useState(false);
  // The confirmation step holds the resolve result awaiting the user's decision
  // on back-fills; `null` means we're still on the pick step.
  const [pending, setPending] = useState<ResolvedPlan | null>(null);
  const [acceptedBackfills, setAcceptedBackfills] = useState<Set<number>>(() => new Set());

  const existingIds = useMemo(
    () => new Set(currentTypes.map((type) => type.id)),
    [currentTypes],
  );

  const catalog = presets.data;

  function toggle(id: string) {
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  async function resolve() {
    setResolving(true);
    try {
      // Catalog order, not Set (click) order — the new types append in the
      // order the picker displayed.
      const presetIds = (catalog?.presets ?? [])
        .map((preset) => preset.id)
        .filter((id) => selected.has(id));
      const result = await resolveTypePresets({
        currentTypes,
        presetIds,
        language,
      });
      const plan: ResolvedPlan = {
        types: result.types ?? [],
        backfills: result.backfills ?? [],
        collisions: result.collisions ?? [],
      };
      // No decisions to make → apply straight away.
      if (plan.backfills.length === 0) {
        applyPlan(plan, new Set());
        return;
      }
      setAcceptedBackfills(new Set(plan.backfills.map((_, index) => index)));
      setPending(plan);
    } catch (error) {
      toast.error(errorMessage(error));
    } finally {
      setResolving(false);
    }
  }

  function applyPlan(plan: ResolvedPlan, accepted: Set<number>) {
    // Apply accepted back-fills onto a copy of the current types, then append the
    // newly resolved types. Everything stays a draft until the editor's Save.
    const merged = currentTypes.map((type) => ({ ...type, fields: [...type.fields] }));
    plan.backfills.forEach((backfill, index) => {
      if (!accepted.has(index)) return;
      const target = merged.find((type) => type.id === backfill.typeId);
      if (target && !target.fields.some((field) => field.field === backfill.field.field)) {
        target.fields.push(backfill.field);
      }
    });
    onApply([...merged, ...plan.types]);
    if (plan.collisions.length > 0) {
      const added = plural(plan.types.length, { one: "# type", other: "# types" });
      const renamed = plan.collisions
        .map((collision) => t`renamed to “${collision.assignedId}”`)
        .join(", ");
      toast.success(t`Added ${added} — ${renamed}`);
    }
    onClose();
  }

  const title = pending ? t`Link your new types` : t`Add built-in types`;
  const description = pending
    ? t`These existing types can link to what you're adding.`
    : t`Robust, ready-to-use types with full metadata matching. Edit or rename everything after adding.`;

  return (
    <Dialog open onOpenChange={(open) => (!open ? onClose() : undefined)}>
      <DialogContent className="flex flex-col gap-0 overflow-hidden p-0 sm:max-h-[85vh] sm:max-w-3xl">
        <DialogHeader className="space-y-0 border-b px-4 py-3 pr-12 text-left">
          <DialogTitle className="min-w-0 truncate">{title}</DialogTitle>
          <DialogDescription className="mt-1">{description}</DialogDescription>
        </DialogHeader>

        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4">
          {pending ? (
            <BackfillStep
              plan={pending}
              accepted={acceptedBackfills}
              onToggle={(index) =>
                setAcceptedBackfills((current) => {
                  const next = new Set(current);
                  if (next.has(index)) next.delete(index);
                  else next.add(index);
                  return next;
                })
              }
            />
          ) : presets.isPending ? (
            <p className="text-sm text-muted-foreground">
              <Trans>Loading built-in types…</Trans>
            </p>
          ) : presets.error ? (
            <Alert>{errorMessage(presets.error)}</Alert>
          ) : (
            <PresetGallery
              catalog={catalog}
              selected={selected}
              existingIds={existingIds}
              onToggle={toggle}
            />
          )}
        </div>

        <DialogFooter className="flex-col gap-2 border-t px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
          {pending ? (
            <>
              <Button type="button" variant="ghost" onClick={() => setPending(null)}>
                <Trans>Back</Trans>
              </Button>
              <Button type="button" onClick={() => applyPlan(pending, acceptedBackfills)}>
                <Plural value={pending.types.length} one="Add # type" other="Add # types" />
              </Button>
            </>
          ) : (
            <>
              <SeedLanguagePicker
                userLanguages={userLanguages}
                value={language}
                onChange={setLanguage}
              />
              <Button
                type="button"
                onClick={resolve}
                disabled={selected.size === 0 || resolving}
              >
                {resolving
                  ? t`Adding…`
                  : selected.size > 0
                    ? t`Continue (${selected.size})`
                    : t`Continue`}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

type ResolvedPlan = {
  types: EntityTypeConfig[];
  backfills: TypePresetBackfill[];
  collisions: { assignedId: string }[];
};

/**
 * The category-grouped preset gallery with the "pairs well with" nudge. Shared by
 * the settings dialog and the onboarding wizard; the parent owns selection state.
 */
export function PresetGallery({
  catalog,
  selected,
  existingIds,
  onToggle,
}: {
  catalog: { presets: TypePresetSummary[]; categories: { id: string; label: string }[] } | undefined;
  selected: Set<string>;
  existingIds: Set<string>;
  onToggle: (id: string) => void;
}) {
  const byCategory = useMemo(() => groupByCategory(catalog?.presets ?? []), [catalog]);

  // Hub types (e.g. Franchise) that the current selection links to but that
  // aren't selected or already present — the "pairs well with" nudge.
  const suggestions = useMemo(() => {
    if (!catalog) return [];
    const wanted = new Set<string>();
    for (const preset of catalog.presets) {
      if (!selected.has(preset.id)) continue;
      for (const target of preset.relationTargets ?? []) {
        if (!selected.has(target) && !existingIds.has(target)) wanted.add(target);
      }
    }
    return catalog.presets.filter((preset) => wanted.has(preset.id));
  }, [catalog, selected, existingIds]);

  return (
    <div className="flex flex-col gap-5">
      {catalog?.categories.map((category) => {
        const items = byCategory.get(category.id) ?? [];
        if (items.length === 0) return null;
        return (
          <section key={category.id} className="flex flex-col gap-2">
            <h3 className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              {category.label}
            </h3>
            <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
              {items.map((preset) => (
                <PresetCard
                  key={preset.id}
                  preset={preset}
                  selected={selected.has(preset.id)}
                  added={existingIds.has(preset.id)}
                  onToggle={() => onToggle(preset.id)}
                />
              ))}
            </div>
          </section>
        );
      })}

      {suggestions.length > 0 ? (
        <section className="flex flex-col gap-2 rounded-md border border-dashed p-3">
          <div className="text-xs font-medium text-muted-foreground">
            <Trans>✨ Pairs well with your picks</Trans>
          </div>
          <div className="flex flex-wrap gap-2">
            {suggestions.map((preset) => (
              <Button
                key={preset.id}
                type="button"
                variant="outline"
                size="sm"
                onClick={() => onToggle(preset.id)}
              >
                <span aria-hidden>{preset.icon}</span>
                {preset.label}
                <span className="text-muted-foreground">— {preset.description}</span>
              </Button>
            ))}
          </div>
        </section>
      ) : null}
    </div>
  );
}

function PresetCard({
  preset,
  selected,
  added,
  onToggle,
}: {
  preset: TypePresetSummary;
  selected: boolean;
  added: boolean;
  onToggle: () => void;
}) {
  return (
    <button
      type="button"
      disabled={added}
      aria-pressed={selected}
      onClick={onToggle}
      className={[
        "flex h-full flex-col items-start gap-1 rounded-md border p-3 text-left transition-colors",
        added
          ? "cursor-default border-dashed opacity-60"
          : selected
            ? "border-primary bg-primary/5 ring-1 ring-primary"
            : "hover:border-foreground/30",
      ].join(" ")}
    >
      <div className="flex w-full items-center gap-2">
        <span className="text-lg" aria-hidden>
          {preset.icon}
        </span>
        <span className="min-w-0 flex-1 truncate font-medium">{preset.label}</span>
        {added ? (
          <Badge variant="secondary">
            <Trans>Added</Trans>
          </Badge>
        ) : selected ? (
          <CheckIcon className="size-4 text-primary" />
        ) : null}
      </div>
      <p className="text-xs text-muted-foreground">{preset.description}</p>
      {(preset.providers?.length ?? 0) > 0 ? (
        <div className="mt-1 flex flex-wrap gap-1">
          {preset.providers.map((provider) => (
            <Badge key={provider.id} variant="outline" className="text-[10px]">
              {provider.label}
            </Badge>
          ))}
        </div>
      ) : null}
    </button>
  );
}

function BackfillStep({
  plan,
  accepted,
  onToggle,
}: {
  plan: ResolvedPlan;
  accepted: Set<number>;
  onToggle: (index: number) => void;
}) {
  const assignedIds = plan.collisions.map((collision) => `“${collision.assignedId}”`).join(", ");
  return (
    <div className="flex flex-col gap-4">
      {plan.collisions.length > 0 ? (
        <Alert>
          <Trans>
            A type with the same name already exists, so {assignedIds} was used instead. You can
            rename it after adding.
          </Trans>
        </Alert>
      ) : null}
      <div className="flex flex-col gap-2">
        {plan.backfills.map((backfill, index) => (
          <button
            key={`${backfill.typeId}-${backfill.field.field}`}
            type="button"
            aria-pressed={accepted.has(index)}
            onClick={() => onToggle(index)}
            className={[
              "flex items-center gap-3 rounded-md border p-3 text-left transition-colors",
              accepted.has(index) ? "border-primary bg-primary/5" : "hover:border-foreground/30",
            ].join(" ")}
          >
            <span
              className={[
                "flex size-5 shrink-0 items-center justify-center rounded border",
                accepted.has(index) ? "border-primary bg-primary text-primary-foreground" : "",
              ].join(" ")}
              aria-hidden
            >
              {accepted.has(index) ? <CheckIcon className="size-3.5" /> : null}
            </span>
            <span className="text-sm">
              <Trans>
                Add a <strong>{backfill.presetLabel}</strong> link to{" "}
                <strong>{backfill.typeLabel}</strong>
              </Trans>
            </span>
          </button>
        ))}
      </div>
    </div>
  );
}

function groupByCategory(presets: TypePresetSummary[]) {
  const map = new Map<string, TypePresetSummary[]>();
  for (const preset of presets) {
    const list = map.get(preset.category) ?? [];
    list.push(preset);
    map.set(preset.category, list);
  }
  return map;
}

/**
 * The single language choice for new types (labels *and* titles), shared by the
 * settings dialog and onboarding. Options are the core's user-language list
 * (endonym labels, `zh` split into 简体/繁體) — the same list as the app's
 * global language picker — and an explicit hint appears for languages the
 * built-in text isn't written in.
 */
export function SeedLanguagePicker({
  userLanguages,
  value,
  onChange,
}: {
  userLanguages: UserLanguage[];
  value: string;
  onChange: (code: string) => void;
}) {
  // Keep the current value selectable even if it isn't in the list (an unusual
  // stored preference), so the control never shows blank.
  const options: UserLanguage[] =
    userLanguages.length > 0 && !userLanguages.some((item) => item.code === value)
      ? [{ code: value, label: value, titleLanguage: value }, ...userLanguages]
      : userLanguages;
  return (
    <div className="flex flex-col gap-1">
      <label className="flex items-center gap-2 text-sm">
        <span className="text-muted-foreground">
          <Trans>Language</Trans>
        </span>
        <Select
          value={value}
          onChange={(event) => onChange(event.target.value)}
          className="h-9 text-base md:text-sm"
        >
          {options.map((item) => (
            <option key={item.code} value={item.code}>
              {item.label}
            </option>
          ))}
        </Select>
      </label>
      {!SEED_TEXT_LANGUAGES.has(value) ? (
        <p className="text-xs text-muted-foreground">
          <Trans>Built-in labels will be in English. Titles still use this language.</Trans>
        </p>
      ) : null}
    </div>
  );
}

/** The initial language for new types: the app's preference when the core offers it, else English. */
export function defaultSeedLanguage(userLanguages: UserLanguage[], preference: string): string {
  return userLanguages.some((option) => option.code === preference) ? preference : "en";
}
