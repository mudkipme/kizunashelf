import { BrowserRouter, Route, Routes } from "react-router-dom";

import { EntityPage } from "@/pages/entity-page";
import { HomePage } from "@/pages/home-page";
import { LibraryPage } from "@/pages/library-page";

export default function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<HomePage />} />
        <Route path="/library" element={<LibraryPage />} />
        <Route path="/entities/:id" element={<EntityPage />} />
      </Routes>
    </BrowserRouter>
  );
}
