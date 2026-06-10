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

  useEffect(() => {
    function reload() {
      void queryClient.invalidateQueries();
    }
    window.addEventListener("kizunashelf-config-saved", reload);
    return () => window.removeEventListener("kizunashelf-config-saved", reload);
  }, [queryClient]);

  if (settings.isPending) {
    return (
      <main className="h-dvh overflow-auto bg-background p-8 text-center text-sm text-muted-foreground">
        Loading
      </main>
    );
  }

  const pathname = location.pathname;
  if (settings.error) {
    return pathname === "/settings" ? <AppRoutes /> : <Navigate to="/settings" replace />;
  }
  if (settings.data && !settings.data.exists && pathname !== "/onboarding") {
    return <Navigate to="/onboarding" replace />;
  }
  if (settings.data?.error && pathname !== "/settings" && pathname !== "/onboarding") {
    return <Navigate to="/settings" replace />;
  }
  if (settings.data?.exists && pathname === "/onboarding" && !settings.data.error) {
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
