import type { ChangeEvent } from "react";
import type { AppController } from "../../app/AppController";
import type { AppState, Page } from "../../app/AppState";
import type { BusyChoice, ModalChoice, TestViewControls, ToastChoice, UpdateChoice } from "../../app/testview/TestViewControls";
import { SelectionPresets } from "../../testview/SelectionPresets";
import type { TestViewPanelSettings, TypeSize } from "../../testview/TestViewPanelSettings";
import { testViewSteamStates, type TestViewSteamState } from "../../testview/TestViewFixtures";
import { IconPreview } from "../components/PixelIcons";

type Option<T extends string> = readonly [value: T, label: string];
type PanelActions = Pick<AppController, "showPage" | "toggleTestView">;

const typeSizes: Option<TypeSize>[] = [["classic", "8 / 16 / 24 px"], ["large", "10 / 18 / 26 px"]];
const pages: Option<Page>[] = [["library", "Tool Library"], ["steamy", "Steamy Friends"]];
const steamStates: Option<TestViewSteamState>[] = testViewSteamStates.map(state => [state, state.replace("-last-confirmed", " (last confirmed)")]);
const selections: Option<string>[] = SelectionPresets.all.map(preset => [preset.name, preset.name]);
const toasts: Option<ToastChoice>[] = [
  ["none", "None"], ["success", "Success"], ["error", "Error"], ["update", "Update with button"], ["stacked", "Error + update stack"],
];
const modals: Option<ModalChoice>[] = [
  ["none", "None"], ["settings", "Settings"], ["update", "Startup update"], ["force", "Force restart confirmation"],
];
const busyStates: Option<BusyChoice>[] = [
  ["none", "Idle"], ["status", "Initial Steam check"], ["restart", "Restarting Steam"], ["confirm-startup", "Loading UI"],
  ["loaded", "Loaded"], ["network", "Changing network"], ["check-update", "Checking updates"], ["install-update", "Downloading update"],
];
const updateStates: Option<UpdateChoice>[] = [["available", "Update available"], ["current", "Up to date"], ["error", "Update error"]];

/** Controls for the UI test view: every page, Steam state, toast, dialog, and busy state. */
export function TestViewPanel({ state, settings, controls, actions }: {
  state: AppState;
  settings: TestViewPanelSettings;
  controls: TestViewControls;
  actions: PanelActions;
}) {
  const { panelOpen, typeSize, selection } = settings;
  return (
    <aside className={`ui-test-panel ${panelOpen ? "" : "collapsed"}`} aria-label="UI test controls">
      <div className="ui-test-heading">
        <IconPreview size={17} />
        <strong className="ui-label">UI test view</strong>
        <button className="small-button" onClick={() => controls.configure({ panelOpen: !panelOpen })}>
          <span className="ui-label">{panelOpen ? "Hide controls" : "Show controls"}</span>
        </button>
      </div>
      {panelOpen && (
        <>
          <p>Simulated actions only. Ctrl+Shift+T exits.</p>
          <Choice label="Text sizes" value={typeSize} options={typeSizes} onChange={typeSize => controls.configure({ typeSize })} />
          <Choice label="Selection" ariaLabel="Selection background" value={selection} options={selections}
            onChange={selection => controls.configure({ selection })} />
          <p className="selection-preview">Drag over this text to preview selection.</p>
          <Choice label="Page" value={state.page} options={pages} onChange={page => actions.showPage(page)} />
          <Choice label="Steam state" value={controls.steamState} options={steamStates} onChange={value => controls.simulateSteam(value)} />
          <Choice label="Toast" value={controls.toast} options={toasts} onChange={value => controls.simulateToast(value)} />
          <Choice label="Modal" value={controls.modal} options={modals} onChange={value => controls.simulateModal(value)} />
          <Choice label="Busy state" value={controls.busy} options={busyStates} onChange={value => controls.simulateBusy(value)} />
          <Choice label="Update state" value={controls.update} options={updateStates} onChange={value => controls.simulateUpdate(value)} />
          <button className="small-button" onClick={() => actions.toggleTestView()}>
            <span className="ui-label">Exit test view</span>
          </button>
        </>
      )}
    </aside>
  );
}

function Choice<T extends string>({ label, ariaLabel = label, value, options, onChange }: {
  label: string;
  ariaLabel?: string;
  value: T;
  options: readonly Option<T>[];
  onChange: (value: T) => void;
}) {
  return (
    <label>
      <span className="ui-label">{label}</span>
      <select aria-label={ariaLabel} value={value} onChange={(event: ChangeEvent<HTMLSelectElement>) => onChange(event.target.value as T)}>
        {options.map(([option, text]) => <option key={option} value={option}>{text}</option>)}
      </select>
    </label>
  );
}
