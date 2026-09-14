import { Alert } from "@/components/ui/alert";
import type { ExternalProviderSummary } from "@/types/api";

export function ExternalSearchErrors({ providers }: { providers: ExternalProviderSummary[] }) {
  const failures = providers.filter((provider) => provider.error);
  if (failures.length === 0) return null;
  return (
    <Alert>
      <ul className="flex flex-col gap-1">
        {failures.map((provider) => (
          <li key={provider.id}>
            <span className="font-medium">{provider.label}</span>: {provider.error}
          </li>
        ))}
      </ul>
    </Alert>
  );
}
