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
import { isDesktopRuntime } from "@/lib/desktop";
import { CalendarPage } from "@/pages/calendar-page";
import { EntityCreatePage } from "@/pages/entity-create-page";
import { EntityEditPage } from "@/pages/entity-edit-page";
import { EntityPage } from "@/pages/entity-page";
import { HomePage } from "@/pages/home-page";
import { LibraryPage } from "@/pages/library-page";
import { OnboardingPage } from "@/pages/onboarding-page";
import { RelationsPage } from "@/pages/relations-page";
import { ReviewPage } from "@/pages/review-page";
import { SettingsPage } from "@/pages/settings-page";
import { StatisticsPage } from "@/pages/statistics-page";

export default function App() {
  return (
    <BrowserRouter>
      <ConfigGate />
    </BrowserRouter>
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
  // `appExists` is always true now (the app config is inline: env on web, the
  // vault switcher on desktop), so vault config presence alone gates readiness.
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
      <Route path="/review" element={<ReviewPage />} />
      <Route path="/review/:queueId" element={<ReviewPage />} />
      <Route path="/statistics" element={<StatisticsPage />} />
      <Route path="/relations" element={<RelationsPage />} />
      <Route path="/entities/new" element={<EntityCreatePage />} />
      <Route path="/entities/:id/edit" element={<EntityEditPage />} />
      <Route path="/entities/:id" element={<EntityPage />} />
    </Routes>
  );
}
