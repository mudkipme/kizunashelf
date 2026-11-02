import { isDesktopRuntime, listVaults } from "@/lib/desktop";

export const desktopVaultsKey = ["desktop-vaults"] as const;

export function desktopVaultsQuery() {
  return {
    queryKey: desktopVaultsKey,
    queryFn: listVaults,
    enabled: isDesktopRuntime(),
  };
}
