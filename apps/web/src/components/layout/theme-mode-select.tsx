import type { MessageDescriptor } from "@lingui/core";
import { msg } from "@lingui/core/macro";
import { useLingui } from "@lingui/react/macro";
import { LaptopIcon, MoonIcon, SunIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { useThemeStore, type ThemeMode } from "@/lib/theme";

const modeLabels: Record<ThemeMode, MessageDescriptor> = {
  system: msg`System theme`,
  light: msg`Light theme`,
  dark: msg`Dark theme`,
};

// Whole sentences per mode instead of composing "Switch to X" from the label
// map, so each locale can phrase the announcement naturally.
const switchLabels: Record<ThemeMode, MessageDescriptor> = {
  system: msg`System theme. Switch to light theme.`,
  light: msg`Light theme. Switch to dark theme.`,
  dark: msg`Dark theme. Switch to system theme.`,
};

const nextMode: Record<ThemeMode, ThemeMode> = {
  system: "light",
  light: "dark",
  dark: "system",
};

export function ThemeModeSelect() {
  const { i18n } = useLingui();
  const mode = useThemeStore((state) => state.mode);
  const setMode = useThemeStore((state) => state.setMode);

  const Icon = mode === "light" ? SunIcon : mode === "dark" ? MoonIcon : LaptopIcon;

  return (
    <Button
      type="button"
      variant="ghost"
      size="icon"
      aria-label={i18n._(switchLabels[mode])}
      title={i18n._(modeLabels[mode])}
      onClick={() => setMode(nextMode[mode])}
    >
      <Icon />
    </Button>
  );
}
