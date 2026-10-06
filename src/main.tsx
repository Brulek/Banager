import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import App from "./App";
import { RootErrorBoundary } from "./components/RootErrorBoundary";
import "./i18n";
import "./index.css";
import { watchAccentInk } from "./lib/accentInk";

const queryClient = new QueryClient();

// Black words on a light accent the user picked, white on the rest
// (decision I21b), worked out again as the window comes back to the front.
watchAccentInk();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      {/* An error in drawing the page shows a line and Reload, rather
          than a blank window. */}
      <RootErrorBoundary>
        <App />
      </RootErrorBoundary>
    </QueryClientProvider>
  </React.StrictMode>,
);
