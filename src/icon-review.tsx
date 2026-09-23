import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource/press-start-2p";
import "@fontsource/dm-sans/400.css";
import "@fontsource/dm-sans/600.css";
import {
  ArrowLeft,
  Check,
  ChevronDown,
  ChevronRight,
  CircleAlert,
  FolderOpen,
  Grid2X2,
  LoaderCircle,
  Power,
  Preview,
  Radio,
  RotateCw,
  Settings2,
  SteamFriendLogo,
  Wifi,
  WifiOff,
  X,
} from "./PixelIcons";
import appIcon from "../assets/app-icon.svg";
import "./icon-review.css";

type Item = { name: string; usedFor: string; icon: React.ReactNode };
type Candidate = { name: string; asset: string; icon?: React.ReactNode };
type CandidateRow = Item & { choices: [Candidate, Candidate, Candidate] };

const iconUrl = (name: string) => `https://unpkg.com/pixelarticons@2.4.1/svg/${name}.svg`;
const choice = (name: string, asset: string): Candidate => ({ name, asset });

type SpinnerVariant = "blocks" | "bars" | "chase";

// Square-pixel adaptations of SVG Spinners' blocks-shuffle-3, bars-scale, and
// blocks-scale patterns (MIT, Utkarsh Verma). The selected chase is shared
// with the application; the orbit and bars remain review-only alternatives.
function SpinnerCandidate({ variant, size }: { variant: SpinnerVariant; size: number }) {
  if (variant === "chase") return <LoaderCircle size={size} />;
  return <svg className={`spinner-candidate spinner-${variant}`} width={size} height={size} viewBox="0 0 24 24" fill="currentColor" shapeRendering="crispEdges" aria-hidden="true">
    {variant === "blocks" && <g className="spinner-orbit"><rect x="2" y="2" width="5" height="5" /><rect x="17" y="2" width="5" height="5" /><rect x="17" y="17" width="5" height="5" /></g>}
    {variant === "bars" && [0, 1, 2, 3, 4].map((i) => <rect className="spinner-bar" key={i} x={i * 5} y="6" width="3" height="12" style={{ animationDelay: `${-i * .12}s` }} />)}
  </svg>;
}

const selected: Item[] = [
  { name: "Steamy Friends mark", usedFor: "Menu, tool heading, library card", icon: <SteamFriendLogo /> },
  { name: "Refresh", usedFor: "Restart Steam", icon: <RotateCw size={32} /> },
  { name: "Library", usedFor: "Tool library menu", icon: <Grid2X2 size={32} /> },
  { name: "Sliders", usedFor: "Settings menu and dialog", icon: <Settings2 size={32} /> },
  { name: "List box", usedFor: "Session activity", icon: <Radio size={32} /> },
  { name: "Stopped", usedFor: "Steam inactive / setup needed", icon: <Power size={32} /> },
  { name: "Sharp folder", usedFor: "Locate steam.exe", icon: <FolderOpen size={32} /> },
];

const pending: CandidateRow[] = [
  { name: "Wi-Fi", usedFor: "Network control / unblocked", icon: <Wifi size={32} />, choices: [choice("Wi-Fi", "wifi"), choice("Signal", "signal"), choice("Full bars", "cellular-signal-3")] },
  { name: "Wi-Fi off", usedFor: "Blocked network · matched to 1A", icon: <WifiOff size={32} />, choices: [{ name: "Paired Wi-Fi off", asset: "rbx-wifi-off", icon: <WifiOff size={32} /> }, choice("Zero bars", "cellular-signal-0"), choice("Power off", "power-off")] },
  { name: "Back arrow", usedFor: "Back to tool library", icon: <ArrowLeft size={32} />, choices: [choice("Arrow left", "arrow-left"), choice("Chevron left", "chevron-left"), choice("Square chevron", "square-chevron-left")] },
  { name: "Right chevron", usedFor: "Open tool card", icon: <ChevronRight size={32} />, choices: [choice("Chevron right", "chevron-right"), choice("Chevron right 2", "chevron-right-2"), choice("Square chevron", "square-chevron-right")] },
  { name: "Down chevron", usedFor: "Expand session activity", icon: <ChevronDown size={32} />, choices: [choice("Chevron down", "chevron-down"), choice("Chevron down 2", "chevron-down-2"), choice("Square chevron", "square-chevron-down")] },
  { name: "Check", usedFor: "Running / ready / success", icon: <Check size={32} />, choices: [choice("Check", "check"), choice("Double check", "check-double"), choice("Checked box", "checkbox-on")] },
  { name: "Alert", usedFor: "Needs attention", icon: <CircleAlert size={32} />, choices: [choice("Square alert", "square-alert"), choice("Square alert · sharp", "square-alert-sharp"), choice("Warning diamond", "warning-diamond")] },
  { name: "Spinner", usedFor: "Working / busy", icon: <LoaderCircle size={32} />, choices: [
    { name: "3-block orbit", asset: "blocks-shuffle-3", icon: <SpinnerCandidate variant="blocks" size={32} /> },
    { name: "5-bar wave", asset: "bars-scale", icon: <SpinnerCandidate variant="bars" size={32} /> },
    { name: "4-block chase", asset: "blocks-scale", icon: <SpinnerCandidate variant="chase" size={32} /> },
  ] },
  { name: "Preview", usedFor: "Browser preview banner", icon: <Preview size={32} />, choices: [choice("Eye", "eye"), choice("Window frame", "window-frame"), choice("Info box", "info-box")] },
  { name: "Close", usedFor: "Dismiss notice / close dialog", icon: <X size={32} />, choices: [choice("Close", "close"), choice("Cross", "cross"), choice("Cancel", "cancel")] },
];

const applied: Record<number, string> = { 1: "A", 2: "paired with 1A", 3: "B", 4: "A", 5: "A", 6: "A", 7: "A", 8: "C", 9: "C", 10: "A" };

function InContextCheck({ letter, asset }: { letter: string; asset: string }) {
  return <div className="context-choice">
    <strong>{letter} in the real status slot</strong>
    <div className="context-card">
      <div className="context-card-title">Restart Steam</div>
      <div className="context-status"><span className="context-status-icon"><img src={iconUrl(asset)} alt="" /></span>Steam is running</div>
    </div>
  </div>;
}

function InContextSpinner({ letter, variant }: { letter: string; variant: SpinnerVariant }) {
  return <div className="spinner-context-choice"><strong>8{letter}</strong><div className="context-status"><span className="context-status-icon"><SpinnerCandidate variant={variant} size={17} /></span>Checking Steam…</div></div>;
}

function Group({ title, items }: { title: string; items: Item[] }) {
  return <section className="review-group">
    <h2>{title}</h2>
    <div className="review-grid">
      {items.map((item) => <div className="review-item" key={item.name}>
        <div className="review-art" aria-hidden="true">{item.icon}</div>
        <div><strong>{item.name}</strong><span>{item.usedFor}</span></div>
      </div>)}
    </div>
  </section>;
}

ReactDOM.createRoot(document.getElementById("icon-review-root")!).render(
  <React.StrictMode>
    <main className="review-shell">
      <header><h1>Icon review</h1><p>Your picks are applied, including 6A and the 8C four-block busy animation.</p></header>
      <section className="candidate-list" aria-label="Icon replacement candidates">
        {pending.map((row, index) => <div className={`candidate-row${applied[index + 1] ? " applied" : ""}`} key={row.name}>
          <div className="candidate-heading"><strong>{index + 1}. {row.name}</strong><span>{row.usedFor}</span>{applied[index + 1] && <em>Applied: {applied[index + 1]}</em>}</div>
          <div className="candidate-options">
            <div className="candidate-option current"><div className="candidate-art" aria-hidden="true">{row.icon}</div><span>Current</span></div>
            {row.choices.map((option, optionIndex) => <div className={`candidate-option${applied[index + 1] === String.fromCharCode(65 + optionIndex) ? " picked" : ""}`} key={option.asset}>
              <div className="candidate-art">{option.icon ?? <img src={iconUrl(option.asset)} alt="" loading="lazy" />}</div>
              <span><b>{String.fromCharCode(65 + optionIndex)}.</b> {option.name}</span>
            </div>)}
          </div>
          {index + 1 === 6 && <div className="context-compare"><InContextCheck letter="6A · check" asset="check" /><InContextCheck letter="6C · checked box" asset="checkbox-on" /></div>}
          {index + 1 === 8 && <><div className="spinner-context"><InContextSpinner letter="A" variant="blocks" /><InContextSpinner letter="B" variant="bars" /><InContextSpinner letter="C" variant="chase" /></div><p className="row-caption">8C is now the app’s busy indicator. These square-pixel animations adapt patterns from the MIT-licensed <a href="https://github.com/n3r4zzurr0/svg-spinners" target="_blank" rel="noreferrer">SVG Spinners</a> collection.</p></>}
        </div>)}
        <div className="candidate-row reserved"><div className="candidate-heading"><strong>11. RBX Tools badge</strong><span>You’ll provide the artwork; leaving the current badge in place.</span></div><div className="candidate-option current"><div className="candidate-art"><img src={appIcon} alt="" /></div><span>Current</span></div></div>
      </section>
      <p className="review-note">Most A/B/C assets are from <a href="https://github.com/halfmage/pixelarticons" target="_blank" rel="noreferrer">Pixelarticons</a> (MIT). Row 2A is our paired variant; row 8 uses square-pixel adaptations of <a href="https://github.com/n3r4zzurr0/svg-spinners" target="_blank" rel="noreferrer">SVG Spinners</a> (MIT).</p>
      <details className="chosen-details"><summary>Already selected · 7 icons</summary><Group title="Selected replacements" items={selected} /></details>
    </main>
  </React.StrictMode>,
);
