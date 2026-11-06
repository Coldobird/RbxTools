import { useEffect, useSyncExternalStore } from "react";
import type { AppController } from "../app/AppController";
import type { AppState } from "../app/AppState";

/**
 * Runs the controller's session while the component is mounted (status
 * polling and update checks on the desktop) and returns its current state.
 */
export function useAppSession(controller: AppController): AppState {
  useEffect(() => {
    controller.start();
    return () => controller.stop();
  }, [controller]);
  return useSyncExternalStore(controller.subscribe, controller.getSnapshot);
}
