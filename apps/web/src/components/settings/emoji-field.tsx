import { lazy, Suspense, useEffect, useState } from "react";
import { Trans, useLingui } from "@lingui/react/macro";
import { SmilePlusIcon, XIcon } from "lucide-react";
import type { EmojiClickData } from "emoji-picker-react";

import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { cn } from "@/lib/utils";

import { Field } from "./settings-controls";

// The picker bundles all emoji data, so it's lazy-loaded and only pulled in
// once the popover is opened — keeping it (and the library) out of the initial
// settings chunk. The panel is a separate module so nothing from
// emoji-picker-react is statically imported here (a type-only import is erased).
const EmojiPickerPanel = lazy(() => import("./emoji-picker-panel"));

/// A single-emoji picker for the entity-type "Icon" field. The value is only
/// ever set from a picker selection (exactly one emoji) or cleared — there is
/// no free-text path, so the stored icon is always one emoji or empty.
export function EmojiField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
}) {
  const { t } = useLingui();
  const [open, setOpen] = useState(false);
  const isDark = useIsDarkTheme();

  function handlePick(data: EmojiClickData) {
    onChange(data.emoji);
    setOpen(false);
  }

  return (
    <Field label={label}>
      <div className="flex items-center gap-1">
        <Popover open={open} onOpenChange={setOpen}>
          <PopoverTrigger asChild>
            <Button
              type="button"
              variant="outline"
              // flex-1 (not w-full): the button's base class is shrink-0, so a
              // 100%-wide trigger plus the remove button overflows the row and
              // horizontally scrolls the page on mobile. A zero basis grows to
              // exactly the space the remove button leaves.
              className="min-w-0 flex-1 justify-start px-3 font-normal"
              aria-label={t`Choose icon`}
            >
              {value ? (
                <span className="text-lg leading-none">{value}</span>
              ) : (
                <span className="flex items-center gap-2 text-muted-foreground">
                  <SmilePlusIcon className="size-4" />
                  <Trans>Choose an emoji</Trans>
                </span>
              )}
            </Button>
          </PopoverTrigger>
          <PopoverContent
            align="start"
            className={cn("w-auto border-none bg-transparent p-0 shadow-none")}
          >
            <Suspense
              fallback={
                <div className="flex h-[350px] w-[350px] items-center justify-center rounded-md border bg-popover text-sm text-muted-foreground">
                  <Trans>Loading</Trans>
                </div>
              }
            >
              <EmojiPickerPanel isDark={isDark} onPick={handlePick} />
            </Suspense>
          </PopoverContent>
        </Popover>
        {value ? (
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="shrink-0"
            aria-label={t`Remove icon`}
            onClick={() => onChange("")}
          >
            <XIcon />
          </Button>
        ) : null}
      </div>
    </Field>
  );
}

// Mirrors the app's manual theme so the picker's light/dark matches the rest of
// the UI even when the user forces a theme against their OS preference. The
// resolved theme lives as a `dark` class on <html> (see lib/theme.ts).
function useIsDarkTheme() {
  const [isDark, setIsDark] = useState(() =>
    document.documentElement.classList.contains("dark"),
  );
  useEffect(() => {
    const root = document.documentElement;
    const update = () => setIsDark(root.classList.contains("dark"));
    update();
    const observer = new MutationObserver(update);
    observer.observe(root, { attributes: true, attributeFilter: ["class"] });
    return () => observer.disconnect();
  }, []);
  return isDark;
}
