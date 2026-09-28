import { I18nProvider } from "@lingui/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { type ReactNode, StrictMode } from "react";
import { MemoryRouter } from "react-router-dom";
import { render as renderComponent } from "vitest-browser-react";

import { HistoryPositionProvider } from "@/hooks/use-history-position";
import { i18n } from "@/lib/i18n";

/**
 * Renders a component inside the providers the app always has above it, so a
 * test mounts a subtree in the same context it lives in for real: Lingui (the
 * macros in every component need it), TanStack Query, and a router for the
 * `Link`s and `useNavigate` calls that reach deep into leaf components.
 *
 * Each call gets a fresh `QueryClient` so cached data can never leak between
 * tests, with retries off — a test asserting on an error state should see it
 * immediately rather than after the app's retry.
 *
 * Returns a promise (React renders concurrently), so call sites `await` it.
 */
export function render(ui: ReactNode, { route = "/" }: { route?: string } = {}) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: 0 },
      mutations: { retry: false },
    },
  });

  function Providers({ children }: { children: ReactNode }) {
    return (
      <I18nProvider i18n={i18n}>
        <QueryClientProvider client={queryClient}>
          <MemoryRouter initialEntries={[route]}>
            <StrictMode>
              <HistoryPositionProvider>{children}</HistoryPositionProvider>
            </StrictMode>
          </MemoryRouter>
        </QueryClientProvider>
      </I18nProvider>
    );
  }

  return renderComponent(ui, { wrapper: Providers });
}
