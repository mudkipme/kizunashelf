import { getStats } from "@kizunashelf/api-contract";
import { create } from "zustand";

import { apiFetch, isAbortError } from "@/api/client";
import type { StatsResponse } from "@/types/api";

type SidebarStatsState = {
  stats?: StatsResponse;
  refreshStats: (signal?: AbortSignal) => Promise<void>;
};

export const useSidebarStatsStore = create<SidebarStatsState>()((set) => ({
  stats: undefined,
  async refreshStats(signal) {
    try {
      const stats = await getStats(undefined, signal ? { signal } : undefined, apiFetch);
      set({ stats });
    } catch (error) {
      if (isAbortError(error)) return;
    }
  },
}));
