import { useEffect } from "react";
import { LaptopIcon, MoonIcon, SunIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { applyThemeMode, useThemeStore, type ThemeMode } from "@/lib/theme";

const modeLabels: Record<ThemeMode, string> = {
  system: "System theme",
  light: "Light theme",
  dark: "Dark theme",
};

const nextMode: Record<ThemeMode, ThemeMode> = {
  system: "light",
  light: "dark",
  dark: "system",
};

export function ThemeModeSelect() {
  const mode = useThemeStore((state) => state.mode);
  const setMode = useThemeStore((state) => state.setMode);

  useEffect(() => {
    applyThemeMode(mode);
    if (mode !== "system") return;

    const query = window.matchMedia("(prefers-color-scheme: dark)");
    const handleChange = () => applyThemeMode("system");
    query.addEventListener("change", handleChange);
    return () => query.removeEventListener("change", handleChange);
  }, [mode]);

  const Icon = mode === "light" ? SunIcon : mode === "dark" ? MoonIcon : LaptopIcon;

  return (
    <Button
      type="button"
      variant="ghost"
      size="icon"
      aria-label={`${modeLabels[mode]}. Switch to ${modeLabels[nextMode[mode]].toLocaleLowerCase()}.`}
      title={modeLabels[mode]}
      onClick={() => setMode(nextMode[mode])}
    >
      <Icon />
    </Button>
  );
}
