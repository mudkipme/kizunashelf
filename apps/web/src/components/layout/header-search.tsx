import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { SearchIcon } from "lucide-react";
import type { KeyboardEvent } from "react";
import { Fragment, useMemo, useRef } from "react";

import { entitiesQuery } from "@/api/queries";
import { EntityCover } from "@/components/assets/entity-cover";
import { EntityTitle } from "@/components/entities/entity-title";
import { Badge } from "@/components/ui/badge";
import {
  Combobox,
  ComboboxContent,
  ComboboxItem,
  ComboboxList,
  ComboboxRawInput,
  ComboboxSeparator,
} from "@/components/ui/combobox";
import { useDebouncedValue } from "@/hooks/use-debounce";
import { relevanceSort } from "@/lib/constants";
import { useTitleLanguage } from "@/lib/language";
import { entityTitle } from "@/lib/title-language";
import type { EntitySummary } from "@/types/api";

// How many prefix/relevance-ranked matches the dropdown previews before the
// "see all results" row. Kept small so the popup stays a quick-jump affordance,
// not a second results list — the full page is one Enter away.
const SUGGESTION_LIMIT = 5;
const SEE_ALL_VALUE = "\u0000see-all";

// base-ui reports why the input value changed; only genuine user edits should
// flow back to the search state. Selecting an item also "changes" the input
// (base-ui fills it with the item label), which we must not treat as a search.
const TYPING_REASONS = new Set(["input-change", "input-paste", "input-clear"]);

type Suggestion =
  | { kind: "entity"; value: string; label: string; entity: EntitySummary }
  | { kind: "all"; value: string; label: string };

/**
 * The header search field with a keyboard-navigable suggestion dropdown. Typing
 * previews up to {@link SUGGESTION_LIMIT} relevance-ranked matches plus a "see
 * all results" row; Arrow keys move through them, Enter on a match opens that
 * entity, and Enter with nothing highlighted runs the full search (the original
 * submit behavior). The field itself stays a controlled mirror of the URL query
 * owned by the parent — this component only adds the dropdown.
 */
export function HeaderSearch({
  search,
  onSearchChange,
  onSubmit,
  onSelectEntity,
  type,
  className,
  placeholder,
  autoFocus = false,
}: {
  search: string;
  onSearchChange: (value: string) => void;
  /** Run the full search for the current input (navigate to the results page). */
  onSubmit: () => void;
  onSelectEntity: (id: string) => void;
  /** Type scope for suggestions (the active library type, or "all"). */
  type: string;
  className?: string;
  placeholder: string;
  autoFocus?: boolean;
}) {
  const { t } = useLingui();
  const language = useTitleLanguage();
  const query = search.trim();
  const debouncedQuery = useDebouncedValue(query, 200);
  // The popup shows whenever there's typed text (immediate), while the fetch
  // itself waits for the debounced query — so the dropdown opens on the first
  // keystroke and fills in as results arrive, instead of waiting out the debounce.
  const hasInput = query.length > 0;
  const enabled = debouncedQuery.length > 0;
  // Tracks the currently highlighted item so a bare Enter (nothing highlighted)
  // can fall through to the full-search submit instead of selecting a row.
  const highlighted = useRef<Suggestion | null>(null);

  const suggestions = useQuery({
    ...entitiesQuery({
      type,
      page: 1,
      pageSize: SUGGESTION_LIMIT,
      sort: relevanceSort,
      titleLanguage: language,
      q: debouncedQuery,
    }),
    enabled,
  });

  const entityItems = useMemo<Suggestion[]>(
    () =>
      (suggestions.data?.items ?? []).map((entity) => ({
        kind: "entity",
        value: entity.id,
        label: entityTitle(entity, language),
        entity,
      })),
    [suggestions.data, language],
  );
  // "See all results" is present as soon as there's input, so the popup never
  // shows empty during the debounce; entity rows fill in once the fetch settles.
  const items = useMemo<Suggestion[]>(
    () =>
      hasInput
        ? [...entityItems, { kind: "all", value: SEE_ALL_VALUE, label: t`See all results` }]
        : [],
    [hasInput, entityItems, t],
  );

  function selectItem(item: Suggestion | null) {
    if (!item) return;
    if (item.kind === "all") onSubmit();
    else onSelectEntity(item.entity.id);
  }

  function handleKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    // IME confirmation Enter must never submit or select.
    if (event.key !== "Enter" || event.nativeEvent.isComposing) return;
    // A highlighted row is handled by base-ui (fires onValueChange); only the
    // "nothing highlighted" case falls through to the full search.
    if (highlighted.current) return;
    event.preventDefault();
    onSubmit();
  }

  const showEmpty = enabled && !suggestions.isFetching && entityItems.length === 0;

  return (
    <Combobox
      items={items}
      value={null}
      onValueChange={selectItem}
      inputValue={search}
      onInputValueChange={(value, details) => {
        if (TYPING_REASONS.has(details.reason)) onSearchChange(value);
      }}
      onItemHighlighted={(item) => {
        highlighted.current = (item as Suggestion | undefined) ?? null;
      }}
      filter={null}
    >
      <div className={className}>
        <SearchIcon className="size-4 shrink-0 text-muted-foreground" />
        <ComboboxRawInput
          placeholder={placeholder}
          aria-label={t`Search library`}
          autoFocus={autoFocus}
          className="min-w-0"
          onKeyDown={handleKeyDown}
        />
      </div>
      {hasInput ? (
        <ComboboxContent className="min-w-72">
          <ComboboxList>
            {(item: Suggestion) =>
              item.kind === "all" ? (
                <Fragment key={item.value}>
                  <ComboboxSeparator />
                  <ComboboxItem value={item} className="pr-2 text-sm text-muted-foreground">
                    <SearchIcon className="size-4 shrink-0" />
                    <span className="min-w-0 flex-1 truncate">
                      <Trans>See all results for “{query}”</Trans>
                    </span>
                  </ComboboxItem>
                </Fragment>
              ) : (
                <ComboboxItem key={item.value} value={item} className="gap-3 pr-2">
                  <EntityCover entity={item.entity} />
                  <span className="flex min-w-0 flex-1 items-center gap-2">
                    <EntityTitle
                      entity={item.entity}
                      language={language}
                      className="truncate text-sm font-medium"
                    />
                    <Badge variant="outline" className="ml-auto shrink-0">
                      {item.entity.typeLabel}
                    </Badge>
                  </span>
                </ComboboxItem>
              )
            }
          </ComboboxList>
          {showEmpty ? (
            <div className="px-3 py-3 text-center text-sm text-muted-foreground">
              <Trans>No matches</Trans>
            </div>
          ) : null}
        </ComboboxContent>
      ) : null}
    </Combobox>
  );
}
