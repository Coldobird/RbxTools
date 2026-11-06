import { useEffect } from "react";
import type { AppController } from "../../app/AppController";

/** Ctrl+Shift+T enters or leaves the UI test view. */
export function useTestViewShortcut(controller: AppController): void {
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.ctrlKey && event.shiftKey && event.code === "KeyT") {
        event.preventDefault();
        controller.toggleTestView();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [controller]);
}
