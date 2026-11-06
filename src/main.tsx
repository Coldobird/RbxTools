import { isTauri } from "@tauri-apps/api/core";
import React from "react";
import ReactDOM from "react-dom/client";
import "./styles/fonts.css";
import "./styles/base.css";
import "./styles/pocket-theme.css";
import "./styles/ui-test-view.css";
import "./styles/typography.css";
import "./styles/alignment.css";
import { AppController } from "./app/AppController";
import { DesktopBridge } from "./platform/DesktopBridge";
import { PageVisibility } from "./platform/PageVisibility";
import { Timings } from "./platform/Timings";
import { TestViewMode } from "./testview/TestViewMode";
import { App } from "./ui/App";

// The composition root: every long-lived object is built here, once.
const timings = new Timings();
timings.start();
timings.markOnce("js.start");

const testView = new TestViewMode(TestViewMode.requestedBy(window.location.search));
const desktop = new DesktopBridge(isTauri(), testView, timings);
timings.sendTo(entry => desktop.logTiming(entry));
const controller = new AppController({ desktop, testView, timings, visibility: new PageVisibility() });

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {/* Snaps anti-aliased pixel-font edges to fully on or off. */}
    <svg width="0" height="0" aria-hidden="true" className="pixel-type-filters">
      <defs>
        <filter id="pixel-type" x="-5%" y="-20%" width="110%" height="140%" colorInterpolationFilters="sRGB">
          <feComponentTransfer>
            <feFuncA type="discrete" tableValues="0 1" />
          </feComponentTransfer>
        </filter>
      </defs>
    </svg>
    <App controller={controller} />
  </React.StrictMode>,
);
