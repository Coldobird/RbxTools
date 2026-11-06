import type { CSSProperties, DragEvent } from "react";
import type { AppController } from "../app/AppController";
import { SettingsView } from "../app/views/SettingsView";
import { SteamToolView } from "../app/views/SteamToolView";
import { ToastView } from "../app/views/ToastView";
import { UpdateView } from "../app/views/UpdateView";
import { SelectionPresets } from "../testview/SelectionPresets";
import type { TestViewPanelSettings } from "../testview/TestViewPanelSettings";
import { ModalFrame } from "./components/ModalFrame";
import { ToastStack } from "./components/ToastStack";
import { Sidebar } from "./layout/Sidebar";
import { ForceRestartModal } from "./modals/ForceRestartModal";
import { SettingsModal } from "./modals/SettingsModal";
import { UpdateModal } from "./modals/UpdateModal";
import { SteamyFriendsPage } from "./pages/SteamyFriendsPage";
import { ToolLibraryPage } from "./pages/ToolLibraryPage";
import { TestViewPanel } from "./testview/TestViewPanel";
import { useTestViewPanelSettings } from "./testview/useTestViewPanelSettings";
import { useTestViewShortcut } from "./testview/useTestViewShortcut";
import { useAppSession } from "./useAppSession";

/** The application window: sidebar, current page, notices, and dialogs. */
export function App({ controller }: { controller: AppController }) {
  const state = useAppSession(controller);
  const panel = useTestViewPanelSettings(controller.testViewControls);
  useTestViewShortcut(controller);
  const steamTool = new SteamToolView(state);

  return (
    <div className="app-shell" data-type-size={state.testView ? panel.typeSize : "large"}
      style={state.testView ? selectionStyle(panel) : undefined} onDragStartCapture={preventControlDrag}>
      {state.testView && <TestViewPanel state={state} settings={panel} controls={controller.testViewControls} actions={controller} />}
      <Sidebar page={state.page} steamTool={steamTool} appVersion={controller.appVersion} actions={controller} />

      <div className="workspace">
        <div className="pocket-landscape" aria-hidden="true" />
        <main>
          {state.page === "library"
            ? <ToolLibraryPage steamTool={steamTool} actions={controller} />
            : <SteamyFriendsPage steamTool={steamTool} actions={controller} />}
        </main>
      </div>

      <ToastStack view={new ToastView(state, controller)} actions={controller} />

      {state.modal && (
        <ModalFrame
          dismissible={controller.canDismissModal}
          closeDisabled={!controller.canCloseModal}
          onDismiss={() => controller.dismissModal()}
          onClose={() => controller.closeModal()}
          ignoreFocusFrom={state.testView ? ".ui-test-panel" : undefined}
        >
          {state.modal === "settings" ? <SettingsModal view={new SettingsView(state, controller)} actions={controller} />
            : state.modal === "update" ? <UpdateModal view={new UpdateView(state)} actions={controller} />
            : <ForceRestartModal networkPaused={steamTool.isNetworkPaused} actions={controller} />}
        </ModalFrame>
      )}
    </div>
  );
}

/** The test view previews other text-selection colours. */
function selectionStyle(panel: TestViewPanelSettings): CSSProperties {
  const preset = SelectionPresets.named(panel.selection);
  return { "--selection-background": preset.background, "--selection-color": preset.color } as CSSProperties;
}

/** Buttons and links are not draggable, so a quick click-drag never drags their text. */
function preventControlDrag(event: DragEvent) {
  if (event.target instanceof Element && event.target.closest('button, a[href], select, option, label, [role="button"]')) {
    event.preventDefault();
  }
}
