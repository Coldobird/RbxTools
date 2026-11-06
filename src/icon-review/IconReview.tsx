import React from "react";
import ReactDOM from "react-dom/client";
import "../styles/fonts.css";
import {
  IconBack,
  IconCheck,
  IconAlert,
  IconFolder,
  IconLibrary,
  IconSpinner,
  IconPreview,
  IconRestart,
  IconSettings,
  SteamFriendLogo,
  IconWifi,
  IconWifiOff,
  IconClose,
} from "../ui/components/PixelIcons";
import appIcon from "../../assets/app-logo.png";
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
  if (variant === "chase") return <IconSpinner size={size} />;
  return <svg className={`spinner-candidate spinner-${variant}`} width={size} height={size} viewBox="0 0 24 24" fill="currentColor" shapeRendering="crispEdges" aria-hidden="true">
    {variant === "blocks" && <g className="spinner-orbit"><rect x="2" y="2" width="5" height="5" /><rect x="17" y="2" width="5" height="5" /><rect x="17" y="17" width="5" height="5" /></g>}
    {variant === "bars" && [0, 1, 2, 3, 4].map((i) => <rect className="spinner-bar" key={i} x={i * 5} y="6" width="3" height="12" style={{ animationDelay: `${-i * .12}s` }} />)}
  </svg>;
}

const selected: Item[] = [
  { name: "Steamy Friends mark", usedFor: "Menu, tool heading, library card", icon: <SteamFriendLogo /> },
  { name: "Refresh", usedFor: "Restart Steam", icon: <IconRestart size={32} /> },
  { name: "Library", usedFor: "Tool library menu", icon: <IconLibrary size={32} /> },
  { name: "Sliders", usedFor: "Settings menu and dialog", icon: <IconSettings size={32} /> },
  { name: "Sharp folder", usedFor: "Locate steam.exe", icon: <IconFolder size={32} /> },
];

const pending: CandidateRow[] = [
  { name: "Wi-Fi", usedFor: "Network control / unblocked", icon: <IconWifi size={32} />, choices: [choice("Wi-Fi", "wifi"), choice("Signal", "signal"), choice("Full bars", "cellular-signal-3")] },
  { name: "Wi-Fi off", usedFor: "Blocked network · matched to 1A", icon: <IconWifiOff size={32} />, choices: [{ name: "Paired Wi-Fi off", asset: "rbx-wifi-off", icon: <IconWifiOff size={32} /> }, choice("Zero bars", "cellular-signal-0"), choice("Power off", "power-off")] },
  { name: "Back arrow", usedFor: "Back to tool library", icon: <IconBack size={32} />, choices: [choice("Arrow left", "arrow-left"), choice("Chevron left", "chevron-left"), choice("Square chevron", "square-chevron-left")] },
  { name: "IconCheck", usedFor: "Running / ready / success", icon: <IconCheck size={32} />, choices: [choice("IconCheck", "check"), choice("Double check", "check-double"), choice("Checked box", "checkbox-on")] },
  { name: "Alert", usedFor: "Needs attention", icon: <IconAlert size={32} />, choices: [choice("Square alert", "square-alert"), choice("Square alert · sharp", "square-alert-sharp"), choice("Warning diamond", "warning-diamond")] },
  { name: "Spinner", usedFor: "Working / busy", icon: <IconSpinner size={32} />, choices: [
    { name: "3-block orbit", asset: "blocks-shuffle-3", icon: <SpinnerCandidate variant="blocks" size={32} /> },
    { name: "5-bar wave", asset: "bars-scale", icon: <SpinnerCandidate variant="bars" size={32} /> },
    { name: "4-block chase", asset: "blocks-scale", icon: <SpinnerCandidate variant="chase" size={32} /> },
  ] },
  { name: "IconPreview", usedFor: "Browser preview banner", icon: <IconPreview size={32} />, choices: [choice("Eye", "eye"), choice("Window frame", "window-frame"), choice("Info box", "info-box")] },
  { name: "Close", usedFor: "Dismiss notice / close dialog", icon: <IconClose size={32} />, choices: [choice("Close", "close"), choice("Cross", "cross"), choice("Cancel", "cancel")] },
];

const applied: Record<number, string> = { 1: "A", 2: "paired with 1A", 3: "B", 4: "A", 5: "A", 6: "C", 7: "C", 8: "A" };

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
  return <div className="spinner-context-choice"><strong>6{letter}</strong><div className="context-status"><span className="context-status-icon"><SpinnerCandidate variant={variant} size={17} /></span>Checking Steam…</div></div>;
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
      <header><h1>Icon review</h1><p>Your picks are applied, including 4A and the 6C four-block busy animation.</p></header>
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
          {index + 1 === 4 && <div className="context-compare"><InContextCheck letter="4A · check" asset="check" /><InContextCheck letter="4C · checked box" asset="checkbox-on" /></div>}
          {index + 1 === 6 && <><div className="spinner-context"><InContextSpinner letter="A" variant="blocks" /><InContextSpinner letter="B" variant="bars" /><InContextSpinner letter="C" variant="chase" /></div><p className="row-caption">6C is now the app’s busy indicator. These square-pixel animations adapt patterns from the MIT-licensed <a href="https://github.com/n3r4zzurr0/svg-spinners" target="_blank" rel="noreferrer">SVG Spinners</a> collection.</p></>}
        </div>)}
        <div className="candidate-row reserved"><div className="candidate-heading"><strong>9. RBX Tools badge</strong><span>Coldobird pixel art</span></div><div className="candidate-option current"><div className="candidate-art"><img src={appIcon} alt="" /></div><span>Current</span></div></div>
      </section>
      <p className="review-note">Most A/B/C assets are from <a href="https://github.com/halfmage/pixelarticons" target="_blank" rel="noreferrer">Pixelarticons</a> (MIT). Row 2A is our paired variant; row 6 uses square-pixel adaptations of <a href="https://github.com/n3r4zzurr0/svg-spinners" target="_blank" rel="noreferrer">SVG Spinners</a> (MIT).</p>
      <details className="chosen-details"><summary>Already selected · 5 icons</summary><Group title="Selected replacements" items={selected} /></details>
    </main>
  </React.StrictMode>,
);
