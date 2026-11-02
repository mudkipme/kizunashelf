import { useLingui } from "@lingui/react/macro";
import { useEffect } from "react";

import { VaultManager } from "@/components/desktop/vault-manager";
import { setWindowTitle } from "@/lib/desktop";

export function VaultManagerPage() {
  const { t } = useLingui();

  useEffect(() => {
    const title = `${t`Vaults`} — KizunaShelf`;
    document.title = title;
    void setWindowTitle(title);
  }, [t]);

  return (
    <main className="h-dvh overflow-auto bg-background p-6 text-foreground">
      <VaultManager />
    </main>
  );
}
