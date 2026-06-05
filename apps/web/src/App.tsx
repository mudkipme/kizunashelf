import { useEffect, useState } from "react";
import {
  BrowserRouter,
  Navigate,
  Route,
  Routes,
  useLocation,
  useParams,
} from "react-router-dom";

import { errorMessage } from "@/api/client";
import { getSettingsConfig } from "@/api/settings";
import { CalendarPage } from "@/pages/calendar-page";
import { EntityCreatePage } from "@/pages/entity-create-page";
import { EntityPage } from "@/pages/entity-page";
import { HomePage } from "@/pages/home-page";
import { LibraryPage } from "@/pages/library-page";
import { OnboardingPage } from "@/pages/onboarding-page";
import { RelationsPage } from "@/pages/relations-page";
import { ReviewPage } from "@/pages/review-page";
import { SettingsPage } from "@/pages/settings-page";
import { StatisticsPage } from "@/pages/statistics-page";
import type { SettingsConfigResponse } from "@/types/config";

export default function App() {
  return (
    <BrowserRouter>
      <ConfigGate />
    </BrowserRouter>
  );
}

function ConfigGate() {
  const location = useLocation();
  const [state, setState] = useState<{
    data?: SettingsConfigResponse;
    loading: boolean;
    error?: string;
  }>({ loading: true });

  useEffect(() => {
    void loadSettings();
    function reload() {
      void loadSettings();
    }
    window.addEventListener("kizunashelf-config-saved", reload);
    return () => window.removeEventListener("kizunashelf-config-saved", reload);
  }, []);

  async function loadSettings() {
    setState({ loading: true });
    try {
      const data = await getSettingsConfig();
      setState({ data, loading: false });
    } catch (error) {
      setState({ loading: false, error: errorMessage(error) });
    }
  }

  if (state.loading) {
    return (
      <main className="h-dvh overflow-auto bg-background p-8 text-center text-sm text-muted-foreground">
        Loading
      </main>
    );
  }

  const pathname = location.pathname;
  if (state.error) {
    return pathname === "/settings" ? <AppRoutes /> : <Navigate to="/settings" replace />;
  }
  if (state.data && !state.data.exists && pathname !== "/onboarding") {
    return <Navigate to="/onboarding" replace />;
  }
  if (state.data?.error && pathname !== "/settings" && pathname !== "/onboarding") {
    return <Navigate to="/settings" replace />;
  }
  if (state.data?.exists && pathname === "/onboarding" && !state.data.error) {
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
      <Route path="/cleanup" element={<Navigate to="/review" replace />} />
      <Route path="/cleanup/:queueId" element={<LegacyCleanupRedirect />} />
      <Route path="/statistics" element={<StatisticsPage />} />
      <Route path="/relations" element={<RelationsPage />} />
      <Route path="/relations/:field" element={<Navigate to="/relations" replace />} />
      <Route path="/relations/:field/:target" element={<LegacyRelationTargetRedirect />} />
      <Route path="/entities/new" element={<EntityCreatePage />} />
      <Route path="/entities/:id" element={<EntityPage />} />
    </Routes>
  );
}

function LegacyCleanupRedirect() {
  const { queueId } = useParams();
  return <Navigate to={`/review/${encodeURIComponent(queueId ?? "")}`} replace />;
}

function LegacyRelationTargetRedirect() {
  const { target } = useParams();
  return <Navigate to={`/entities/${encodeURIComponent(target ?? "")}`} replace />;
}
