import { create } from "zustand";
import { persist } from "zustand/middleware";

/**
 * The sidebar's width and collapsed state.
 *
 * Per-device like the theme and language preferences, and for the same reason:
 * how wide a navigation column should be is a property of the screen it is
 * being read on, not of the vault. It never goes near the vault config.
 */
export const sidebarStorageKey = "kizunashelf.sidebar.v1";

/** Narrow enough to still fit the longest destination label, and no narrower. */
export const SIDEBAR_MIN_WIDTH = 180;
export const SIDEBAR_MAX_WIDTH = 420;
export const SIDEBAR_DEFAULT_WIDTH = 224;

export function clampSidebarWidth(width: number): number {
  if (!Number.isFinite(width)) return SIDEBAR_DEFAULT_WIDTH;
  return Math.min(SIDEBAR_MAX_WIDTH, Math.max(SIDEBAR_MIN_WIDTH, Math.round(width)));
}

type SidebarState = {
  width: number;
  collapsed: boolean;
  setWidth: (width: number) => void;
  setCollapsed: (collapsed: boolean) => void;
  toggle: () => void;
  reset: () => void;
};

export const useSidebarStore = create<SidebarState>()(
  persist(
    (set) => ({
      width: SIDEBAR_DEFAULT_WIDTH,
      collapsed: false,
      setWidth: (width) => set({ width: clampSidebarWidth(width) }),
      setCollapsed: (collapsed) => set({ collapsed }),
      toggle: () => set((state) => ({ collapsed: !state.collapsed })),
      reset: () => set({ width: SIDEBAR_DEFAULT_WIDTH }),
    }),
    {
      name: sidebarStorageKey,
      partialize: (state) => ({ width: state.width, collapsed: state.collapsed }),
      // Re-clamped on the way in, not just on the way out: a width written by an
      // older build, a hand-edited value, or one saved on a much wider display
      // must not be able to push the content column off screen.
      merge: (persisted, current) => {
        const stored = persisted as Partial<SidebarState> | undefined;
        return {
          ...current,
          width: clampSidebarWidth(stored?.width ?? SIDEBAR_DEFAULT_WIDTH),
          collapsed: stored?.collapsed === true,
        };
      },
    },
  ),
);
