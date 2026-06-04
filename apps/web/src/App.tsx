import { BrowserRouter, Navigate, Route, Routes, useParams } from "react-router-dom";

import { CalendarPage } from "@/pages/calendar-page";
import { EntityPage } from "@/pages/entity-page";
import { HomePage } from "@/pages/home-page";
import { LibraryPage } from "@/pages/library-page";
import { RelationFieldPage } from "@/pages/relation-field-page";
import { RelationTargetPage } from "@/pages/relation-target-page";
import { RelationsPage } from "@/pages/relations-page";
import { ReviewPage } from "@/pages/review-page";
import { StatisticsPage } from "@/pages/statistics-page";

export default function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<HomePage />} />
        <Route path="/library" element={<LibraryPage />} />
        <Route path="/calendar" element={<CalendarPage />} />
        <Route path="/review" element={<ReviewPage />} />
        <Route path="/review/:queueId" element={<ReviewPage />} />
        <Route path="/cleanup" element={<Navigate to="/review" replace />} />
        <Route path="/cleanup/:queueId" element={<LegacyCleanupRedirect />} />
        <Route path="/statistics" element={<StatisticsPage />} />
        <Route path="/relations" element={<RelationsPage />} />
        <Route path="/relations/:field" element={<RelationFieldPage />} />
        <Route path="/relations/:field/:target" element={<RelationTargetPage />} />
        <Route path="/entities/:id" element={<EntityPage />} />
      </Routes>
    </BrowserRouter>
  );
}

function LegacyCleanupRedirect() {
  const { queueId } = useParams();
  return <Navigate to={`/review/${encodeURIComponent(queueId ?? "")}`} replace />;
}
