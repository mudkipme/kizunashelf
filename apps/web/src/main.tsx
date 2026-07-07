import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { I18nProvider } from "@lingui/react";
import { QueryClientProvider } from "@tanstack/react-query";

import { queryClient } from "@/api/query-client";
import { Toaster } from "@/components/ui/sonner";
import App from "./App";
import { i18n } from "./lib/i18n";
import { initializeTheme } from "./lib/theme";
import "./index.css";

initializeTheme();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <I18nProvider i18n={i18n}>
      <QueryClientProvider client={queryClient}>
        <App />
        <Toaster />
      </QueryClientProvider>
    </I18nProvider>
  </StrictMode>,
);
