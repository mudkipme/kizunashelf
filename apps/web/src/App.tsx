import { useQuery } from "@tanstack/react-query";
import { lazy, Suspense, useEffect, useRef } from "react";
import { BrowserRouter, Navigate, Route, Routes, useLocation } from "react-router-dom";

import { settingsConfigQuery } from "@/api/queries";
import { ErrorBoundary } from "@/components/error-boundary";
import { AppShellFallback } from "@/components/layout/app-frame";
import { VaultChangeSync } from "@/components/vault-change-sync";
import { HistoryPositionProvider } from "@/hooks/use-history-position";
import { isDesktopRuntime } from "@/lib/desktop";
import { activateUiLocale } from "@/lib/i18n";
import { useUiLocale } from "@/lib/language";
import { useSystemThemeSync } from "@/lib/theme";
import { useVaultSession } from "@/lib/vault-session";

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
const HomePage = lazy(() => import("@/pages/home-page").then((m) => ({ default: m.HomePage })));
const LibraryPage = lazy(() =>
  import("@/pages/library-page").then((m) => ({ default: m.LibraryPage })),
);
const ListDetailPage = lazy(() =>
  import("@/pages/list-detail-page").then((m) => ({ default: m.ListDetailPage })),
);
const ListsPage = lazy(() => import("@/pages/lists-page").then((m) => ({ default: m.ListsPage })));
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
  const vaultSession = useVaultSession();
  return (
    <BrowserRouter>
      <LocaleSync />
      <HistoryPositionProvider>
        <RoutedErrorBoundary key={vaultSession} />
      </HistoryPositionProvider>
    </BrowserRouter>
  );
}

// Applies the language preference outside React state: loads/activates the UI
// message catalog and stamps `<html lang>` (browsers pick Han glyph variants by
// the declared language, so a zh-Hant UI must not render under `lang="en"`).
function LocaleSync() {
  const uiLocale = useUiLocale();
  useSystemThemeSync();
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
  const settings = useQuery(settingsConfigQuery());
  const pathname = location.pathname;
  const opened = useRef(false);
  if (settings.data?.vaultExists && !settings.data.error) opened.current = true;

  // Initial failures route to setup. Once opened, keep the mounted screens and
  // drafts through background failures; their reads/writes surface the problem.
  // A vault switch resets this gate through the session key above.
  if (isDesktopRuntime()) {
    if (!settings.data) {
      return pathname === "/onboarding" ? <AppRoutes /> : <Navigate to="/onboarding" replace />;
    }
  } else if (settings.isPending) {
    return <AppShellFallback />;
  } else if (settings.error && !settings.data) {
    // On the web a settings error is a real server error → settings page.
    return pathname === "/settings" ? <AppRoutes /> : <Navigate to="/settings" replace />;
  }
  // The app config is always present (inline: env on web, the vault switcher on
  // desktop, @AppStorage on iOS), so vault config presence alone gates readiness.
  const configReady = Boolean(settings.data?.vaultExists);
  if (!opened.current && settings.data && !configReady && pathname !== "/onboarding") {
    return <Navigate to="/onboarding" replace />;
  }
  if (
    !opened.current &&
    settings.data?.error &&
    pathname !== "/settings" &&
    pathname !== "/onboarding"
  ) {
    return <Navigate to="/settings" replace />;
  }
  if (configReady && pathname === "/onboarding" && !settings.data?.error) {
    return <Navigate to="/" replace />;
  }

  return (
    <>
      <VaultChangeSync key={settings.data?.app?.vaultRoot ?? "vault"} />
      <AppRoutes key={settings.data?.app?.vaultRoot ?? "vault"} />
    </>
  );
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

// Shown while a lazily-loaded page chunk is fetched. The same shell the
// ConfigGate shows, so arriving on a route through either path is one
// continuous frame rather than two different loading screens.
function RouteFallback() {
  const location = useLocation();
  // Onboarding is the one page that renders outside the app frame — it has no
  // vault to put in a sidebar yet — so showing chrome here would flash a
  // sidebar the page itself then takes away.
  if (location.pathname === "/onboarding") {
    return <main className="h-dvh bg-background" />;
  }
  return <AppShellFallback />;
}
