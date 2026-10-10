import React from "react";
import ReactDOM from "react-dom/client";
import "./fonts.css";
import App from "./App";
import "./styles.css";
import "./pocket-theme.css";
import "./ui-test-view.css";
import "./typography.css";
import "./alignment.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <svg width="0" height="0" aria-hidden="true" className="pixel-type-filters">
      <defs>
        <filter id="pixel-type" x="-5%" y="-20%" width="110%" height="140%" colorInterpolationFilters="sRGB">
          <feComponentTransfer>
            <feFuncA type="discrete" tableValues="0 1" />
          </feComponentTransfer>
        </filter>
      </defs>
    </svg>
    <App />
  </React.StrictMode>,
);
