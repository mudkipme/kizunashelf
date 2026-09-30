import { I18nProvider } from "@lingui/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createContext, type ReactNode, StrictMode, useContext } from "react";
import { createMemoryRouter, RouterProvider } from "react-router-dom";
import { render as renderComponent } from "vitest-browser-react";

import { HistoryPositionProvider } from "@/hooks/use-history-position";
import { i18n } from "@/lib/i18n";

const TestContent = createContext<ReactNode>(null);

function RoutedContent() {
  return <HistoryPositionProvider>{useContext(TestContent)}</HistoryPositionProvider>;
}

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
export function render(
  ui: ReactNode,
  {
    route = "/",
    initialEntries = [route],
    initialIndex,
  }: { route?: string; initialEntries?: string[]; initialIndex?: number } = {},
) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: 0 },
      mutations: { retry: false },
    },
  });
  const router = createMemoryRouter([{ path: "*", element: <RoutedContent /> }], {
    initialEntries,
    initialIndex,
  });

  function Providers({ children }: { children: ReactNode }) {
    return (
      <I18nProvider i18n={i18n}>
        <QueryClientProvider client={queryClient}>
          <StrictMode>
            <TestContent.Provider value={children}>
              <RouterProvider router={router} />
            </TestContent.Provider>
          </StrictMode>
        </QueryClientProvider>
      </I18nProvider>
    );
  }

  return renderComponent(ui, { wrapper: Providers });
}
