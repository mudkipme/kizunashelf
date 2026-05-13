import { BrowserRouter, Route, Routes } from "react-router-dom";

import { EntityPage } from "@/pages/entity-page";
import { HomePage } from "@/pages/home-page";
import { LibraryPage } from "@/pages/library-page";
import { RelationFieldPage } from "@/pages/relation-field-page";
import { RelationTargetPage } from "@/pages/relation-target-page";
import { RelationsPage } from "@/pages/relations-page";

export default function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<HomePage />} />
        <Route path="/library" element={<LibraryPage />} />
        <Route path="/relations" element={<RelationsPage />} />
        <Route path="/relations/:field" element={<RelationFieldPage />} />
        <Route path="/relations/:field/:target" element={<RelationTargetPage />} />
        <Route path="/entities/:id" element={<EntityPage />} />
      </Routes>
    </BrowserRouter>
  );
}
