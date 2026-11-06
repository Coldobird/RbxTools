import { useSyncExternalStore } from "react";
import type { TestViewControls } from "../../app/testview/TestViewControls";
import type { TestViewPanelSettings } from "../../testview/TestViewPanelSettings";

/** The UI test view panel's settings. */
export function useTestViewPanelSettings(controls: TestViewControls): TestViewPanelSettings {
  return useSyncExternalStore(controls.settings.subscribe, controls.settings.getSnapshot);
}
