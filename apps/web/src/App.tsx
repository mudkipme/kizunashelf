import { lazy, Suspense, useEffect } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  BrowserRouter,
  Navigate,
  Route,
  Routes,
  useLocation,
} from "react-router-dom";

import { settingsConfigQuery } from "@/api/queries";
import { ErrorBoundary } from "@/components/error-boundary";
import { isDesktopRuntime } from "@/lib/desktop";
import { activateUiLocale } from "@/lib/i18n";
import { useUiLocale } from "@/lib/language";

// Pages are lazy-loaded so each route ships as its own chunk; heavy
// page-specific deps (markdown, lightbox, day-picker, dnd-kit) then only load
// on the routes that use them instead of bloating the initial bundle.
const ActivityPage = lazy(() =>
  import("@/pages/activity-page").then((m) => ({ default: m.ActivityPage })),
);
const CalendarPage = lazy(() =>
  import("@/pages/calendar-page").then((m) => ({ default: m.CalendarPage })),
);
const EntityCreatePage = lazy(() =>
  import("@/pages/entity-create-page").then((m) => ({ default: m.EntityCreatePage })),
);
const ImportWizardPage = lazy(() =>
  import("@/pages/import-wizard-page").then((m) => ({ default: m.ImportWizardPage })),
);
const QuickCapturePage = lazy(() =>
  import("@/pages/quick-capture-page").then((m) => ({ default: m.QuickCapturePage })),
);
const EntityEditPage = lazy(() =>
  import("@/pages/entity-edit-page").then((m) => ({ default: m.EntityEditPage })),
);
const EntityPage = lazy(() =>
  import("@/pages/entity-page").then((m) => ({ default: m.EntityPage })),
);
const HomePage = lazy(() =>
  import("@/pages/home-page").then((m) => ({ default: m.HomePage })),
);
const LibraryPage = lazy(() =>
  import("@/pages/library-page").then((m) => ({ default: m.LibraryPage })),
);
const ListDetailPage = lazy(() =>
  import("@/pages/list-detail-page").then((m) => ({ default: m.ListDetailPage })),
);
const ListsPage = lazy(() =>
  import("@/pages/lists-page").then((m) => ({ default: m.ListsPage })),
);
const NotFoundPage = lazy(() =>
  import("@/pages/not-found-page").then((m) => ({ default: m.NotFoundPage })),
);
const OnboardingPage = lazy(() =>
  import("@/pages/onboarding-page").then((m) => ({ default: m.OnboardingPage })),
);
const ReviewPage = lazy(() =>
  import("@/pages/review-page").then((m) => ({ default: m.ReviewPage })),
);
const SettingsPage = lazy(() =>
  import("@/pages/settings-page").then((m) => ({ default: m.SettingsPage })),
);
const SmartListPage = lazy(() =>
  import("@/pages/smart-list-page").then((m) => ({ default: m.SmartListPage })),
);
const StatisticsPage = lazy(() =>
  import("@/pages/statistics-page").then((m) => ({ default: m.StatisticsPage })),
);

export default function App() {
  return (
    <BrowserRouter>
      <LocaleSync />
      <RoutedErrorBoundary />
    </BrowserRouter>
  );
}

// Applies the language preference outside React state: loads/activates the UI
// message catalog and stamps `<html lang>` (browsers pick Han glyph variants by
// the declared language, so a zh-Hant UI must not render under `lang="en"`).
function LocaleSync() {
  const uiLocale = useUiLocale();
  useEffect(() => {
    document.documentElement.lang = uiLocale;
    void activateUiLocale(uiLocale);
  }, [uiLocale]);
  return null;
}

// Wraps the app in an error boundary keyed on the route, so a render-time throw
// degrades to a recoverable message and navigating away clears it.
function RoutedErrorBoundary() {
  const location = useLocation();
  return (
    <ErrorBoundary resetKey={location.pathname}>
      <ConfigGate />
    </ErrorBoundary>
  );
}

function ConfigGate() {
  const location = useLocation();
  const queryClient = useQueryClient();
  const settings = useQuery(settingsConfigQuery());
  const pathname = location.pathname;

  useEffect(() => {
    function reload() {
      void queryClient.invalidateQueries();
    }
    window.addEventListener("kizunashelf-config-saved", reload);
    return () => window.removeEventListener("kizunashelf-config-saved", reload);
  }, [queryClient]);

  // Desktop replies 503 until a vault is open. That error never resolves on its
  // own, so route to onboarding for any non-success state (error *or* the
  // pending blips of a background refetch). Gating on `isPending`/`error`
  // individually would flip the onboarding route in and out as the query
  // oscillates, and each remount re-fires the request — an endless loop. Once a
  // vault is opened the query succeeds and we fall through to the checks below.
  if (isDesktopRuntime()) {
    if (settings.status !== "success") {
      return pathname === "/onboarding" ? <AppRoutes /> : <Navigate to="/onboarding" replace />;
    }
  } else if (settings.isPending) {
    return (
      <main className="h-dvh overflow-auto bg-background p-8 text-center text-sm text-muted-foreground">
        Loading
      </main>
    );
  } else if (settings.error) {
    // On the web a settings error is a real server error → settings page.
    return pathname === "/settings" ? <AppRoutes /> : <Navigate to="/settings" replace />;
  }
  // The app config is always present (inline: env on web, the vault switcher on
  // desktop, @AppStorage on iOS), so vault config presence alone gates readiness.
  const configReady = Boolean(settings.data?.vaultExists);
  if (settings.data && !configReady && pathname !== "/onboarding") {
    return <Navigate to="/onboarding" replace />;
  }
  if (settings.data?.error && pathname !== "/settings" && pathname !== "/onboarding") {
    return <Navigate to="/settings" replace />;
  }
  if (configReady && pathname === "/onboarding" && !settings.data?.error) {
    return <Navigate to="/" replace />;
  }

  return <AppRoutes />;
}

function AppRoutes() {
  return (
    <Suspense fallback={<RouteFallback />}>
      <Routes>
      <Route path="/" element={<HomePage />} />
      <Route path="/onboarding" element={<OnboardingPage />} />
      <Route path="/settings" element={<SettingsPage />} />
      <Route path="/library" element={<LibraryPage />} />
      <Route path="/calendar" element={<CalendarPage />} />
      <Route path="/activity" element={<ActivityPage />} />
      <Route path="/review" element={<ReviewPage />} />
      <Route path="/review/:queueId" element={<ReviewPage />} />
      <Route path="/statistics" element={<StatisticsPage />} />
      <Route path="/lists" element={<ListsPage />} />
      <Route path="/lists/smart/:id" element={<SmartListPage />} />
      <Route path="/lists/:id" element={<ListDetailPage />} />
      <Route path="/entities/new" element={<QuickCapturePage />} />
      <Route path="/entities/new/manual" element={<EntityCreatePage />} />
      <Route path="/entities/import" element={<ImportWizardPage />} />
      <Route path="/entities/:id/edit" element={<EntityEditPage />} />
      <Route path="/entities/:id" element={<EntityPage />} />
        <Route path="*" element={<NotFoundPage />} />
      </Routes>
    </Suspense>
  );
}

// Shown while a lazily-loaded page chunk is fetched. Mirrors the ConfigGate
// loading state so the transition reads as one continuous "Loading".
function RouteFallback() {
  return (
    <main className="h-dvh overflow-auto bg-background p-8 text-center text-sm text-muted-foreground">
      Loading
    </main>
  );
}
