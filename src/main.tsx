import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { Chooser } from "./components/Chooser";

// The same bundle serves two windows: the app, and the small account picker that opens
// when a Google sign-in link arrives (index.html?chooser=<url>).
const params = new URLSearchParams(window.location.search);
const chooserUrl = params.get("chooser");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {chooserUrl ? <Chooser url={chooserUrl} /> : <App />}
  </React.StrictMode>,
);
