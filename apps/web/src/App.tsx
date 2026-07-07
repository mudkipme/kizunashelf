import { useEffect } from "react";
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
import { ActivityPage } from "@/pages/activity-page";
import { CalendarPage } from "@/pages/calendar-page";
import { EntityCreatePage } from "@/pages/entity-create-page";
import { ImportWizardPage } from "@/pages/import-wizard-page";
import { QuickCapturePage } from "@/pages/quick-capture-page";
import { EntityEditPage } from "@/pages/entity-edit-page";
import { EntityPage } from "@/pages/entity-page";
import { HomePage } from "@/pages/home-page";
import { LibraryPage } from "@/pages/library-page";
import { ListDetailPage } from "@/pages/list-detail-page";
import { ListsPage } from "@/pages/lists-page";
import { OnboardingPage } from "@/pages/onboarding-page";
import { ReviewPage } from "@/pages/review-page";
import { SettingsPage } from "@/pages/settings-page";
import { StatisticsPage } from "@/pages/statistics-page";

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
      <Route path="/lists/:id" element={<ListDetailPage />} />
      <Route path="/entities/new" element={<QuickCapturePage />} />
      <Route path="/entities/new/manual" element={<EntityCreatePage />} />
      <Route path="/entities/import" element={<ImportWizardPage />} />
      <Route path="/entities/:id/edit" element={<EntityEditPage />} />
      <Route path="/entities/:id" element={<EntityPage />} />
    </Routes>
  );
}
