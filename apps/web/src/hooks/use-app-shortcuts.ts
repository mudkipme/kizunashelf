import { useEffect, useRef } from "react";
import { useNavigate } from "react-router-dom";

import { navDestinations } from "@/components/layout/nav-destinations";
import { isDesktopRuntime } from "@/lib/desktop";
import {
  backChord,
  destinationChord,
  forwardChord,
  matchesChord,
  paletteChord,
  searchChord,
  settingsChord,
} from "@/lib/shortcuts";

/**
 * The app's global keyboard shortcuts, bound once at the shell.
 *
 * Listening on `window` rather than gating on what has focus is deliberate:
 * every chord here carries the platform modifier, and a modified chord is
 * expected to work from inside a text field — ⌘K while typing a search opens
 * the palette in every app that has one.
 *
 * Two are bound in the desktop shell only, where no browser owns them first.
 * See `destinationChord` for why that trade lands the way it does.
 */
export function useAppShortcuts({
  paletteOpen,
  onTogglePalette,
  onFocusSearch,
}: {
  paletteOpen: boolean;
  onTogglePalette: () => void;
  onFocusSearch: () => void;
}) {
  const navigate = useNavigate();
  // Held in a ref so the listener is bound once instead of being torn down and
  // re-added on every render of the shell.
  const handlers = useRef({ paletteOpen, onTogglePalette, onFocusSearch });
  handlers.current = { paletteOpen, onTogglePalette, onFocusSearch };

  useEffect(() => {
    function handle(event: KeyboardEvent) {
      // Something nearer the event already claimed it — a dialog's own Escape,
      // a field's own chord.
      if (event.defaultPrevented) return;

      if (matchesChord(event, paletteChord)) {
        event.preventDefault();
        handlers.current.onTogglePalette();
        return;
      }
      // With the palette open it owns the keyboard: focusing the field behind
      // it or navigating out from under it would both be wrong.
      if (handlers.current.paletteOpen) return;

      if (matchesChord(event, searchChord)) {
        event.preventDefault();
        handlers.current.onFocusSearch();
        return;
      }
      if (matchesChord(event, settingsChord)) {
        event.preventDefault();
        navigate("/settings");
        return;
      }
      if (matchesChord(event, backChord)) {
        event.preventDefault();
        navigate(-1);
        return;
      }
      if (matchesChord(event, forwardChord)) {
        event.preventDefault();
        navigate(1);
        return;
      }

      if (!isDesktopRuntime()) return;
      const index = navDestinations.findIndex((_, position) => {
        const chord = destinationChord(position);
        return chord ? matchesChord(event, chord) : false;
      });
      if (index >= 0) {
        event.preventDefault();
        navigate(navDestinations[index].to);
      }
    }

    window.addEventListener("keydown", handle);
    return () => window.removeEventListener("keydown", handle);
  }, [navigate]);
}
