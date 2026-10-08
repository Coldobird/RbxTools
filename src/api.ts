import { invoke, isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { isUiTestActive, requireLiveUi } from "./uiTestView";

export interface Status {
  steamPath: string | null;
  steamRunning: boolean;
  steamOnline: boolean | null;
  reconnectError?: string | null;
  targetPath: string | null;
  blocked: boolean;
  elevated: boolean;
}

export interface AvailableUpdate {
  version: string;
}

export interface UpdateCheck {
  update: AvailableUpdate | null;
}

export const desktop = isTauri();
const previewStatus: Status = {
  steamPath: null,
  steamRunning: false,
  steamOnline: false,
  targetPath: null,
  blocked: false,
  elevated: false,
};

export async function getStatus(): Promise<Status> {
  return desktop && !isUiTestActive() ? invoke("get_status") : previewStatus;
}

export async function pickExecutable(
  kind: "steam" | "target",
): Promise<Status | null> {
  requireLiveUi();
  if (!desktop)
    throw new Error(
      "Open the desktop app to choose an executable. This browser preview cannot change your computer.",
    );
  const path = await open({
    multiple: false,
    directory: false,
    title:
      kind === "steam"
        ? "Locate steam.exe"
        : "Locate steam.exe (the NetLimiter target)",
    filters: [{ name: "Windows application", extensions: ["exe"] }],
  });
  if (!path) return null;
  return invoke("set_path", { kind, path });
}

export async function restartSteam(force: boolean): Promise<string> {
  requireLiveUi();
  if (!desktop)
    throw new Error("Restart is available in the installed desktop app.");
  return invoke("restart_steam", { force });
}

export async function setBlocked(blocked: boolean): Promise<Status> {
  requireLiveUi();
  if (!desktop)
    throw new Error(
      "Network controls are available in the installed desktop app.",
    );
  return invoke("set_blocked", { blocked });
}

export async function makeSpacewarPrivate(): Promise<string> {
  requireLiveUi();
  if (!desktop)
    throw new Error("Spacewar privacy is available in the installed desktop app.");
  return invoke("make_spacewar_private");
}

export async function getUpdate(): Promise<UpdateCheck> {
  return desktop && !isUiTestActive()
    ? invoke("get_github_update")
    : { update: null };
}

export async function installUpdate(): Promise<void> {
  requireLiveUi();
  if (!desktop) throw new Error("Updates are available in the desktop app.");
  return invoke("install_github_update");
}

export function fileName(path: string | null): string {
  return path?.split(/[\\/]/).pop() || "No executable selected";
}
