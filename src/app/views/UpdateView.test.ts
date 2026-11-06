import { expect, it } from "vitest";
import { RunningActions, type ActionName } from "../../domain/RunningActions";
import { initialAppState, type UpdateState } from "../AppState";
import { UpdateView } from "./UpdateView";

function view(update: Partial<UpdateState>, ...busy: ActionName[]): UpdateView {
  const state = initialAppState(false, false);
  return new UpdateView({ ...state, running: RunningActions.of(...busy), update: { ...state.update, ...update } });
}

it("invites a check until an update is found", () => {
  expect(view({})).toMatchObject({ settingsTitle: "Updates", settingsButtonLabel: "Check for updates", canInstall: false });
  expect(view({}, "check-update").settingsButtonLabel).toBe("Checking…");
});

it("offers an available update and shows its download", () => {
  const available = { available: { version: "2.0.0" } };
  expect(view(available)).toMatchObject({ settingsTitle: "Update v2.0.0 ready", installButtonLabel: "Update", canInstall: true });
  expect(view(available, "install-update")).toMatchObject({ isInstalling: true, installButtonLabel: "Downloading…", canInstall: false });
});

it("prefers an error, then feedback, over the default description", () => {
  expect(view({ error: "GitHub is down.", feedback: "Up to date." }).settingsDescription).toBe("GitHub is down.");
  expect(view({ feedback: "Up to date." }).settingsDescription).toBe("Up to date.");
});
