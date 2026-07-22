import { create } from "zustand";
import { persist } from "zustand/middleware";

type QuickCaptureTypeState = {
  lastType?: string;
  setLastType: (type: string) => void;
};

export const quickCaptureTypeStorageKey = "kizunashelf.quickCaptureType.v1";

/// The last entity type searched in Quick Capture. Every search is scoped to
/// one type (the core has no cross-type search), so entry points without a type
/// of their own (home, the all-types library view) reuse the previous choice.
export const useQuickCaptureTypeStore = create<QuickCaptureTypeState>()(
  persist(
    (set) => ({
      lastType: undefined,
      setLastType(type) {
        set({ lastType: type });
      },
    }),
    {
      name: quickCaptureTypeStorageKey,
      partialize: (state) => ({ lastType: state.lastType }),
    },
  ),
);
