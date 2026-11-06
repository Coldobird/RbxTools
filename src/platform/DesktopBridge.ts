import { Channel, invoke, type InvokeArgs } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import tauriConfig from "../../src-tauri/tauri.conf.json";
import { SteamStatus, type SteamSnapshot } from "../domain/SteamStatus";
import type { AvailableUpdate, Desktop } from "./Desktop";
import { DesktopError } from "./DesktopError";
import type { TimingEntry, Timings } from "./Timings";

/** Blocks system calls while the UI test view is on. */
export interface TestViewGuard {
  readonly isActive: boolean;
}

const STATUS_TIMEOUT_MS = 8_000;
const BUSY_RETRY_MS = 30_000;
const BUSY_RETRY_INTERVAL_MS = 500;

/**
 * The only door between the interface and the computer: every Tauri command,
 * including the timing log.
 * Outside the desktop app, or in the UI test view, it reads nothing real and
 * changes nothing.
 */
export class DesktopBridge implements Desktop {
  readonly appVersion = tauriConfig.version;

  constructor(
    readonly isDesktop: boolean,
    private readonly testView: TestViewGuard,
    private readonly timings: Timings,
  ) {}

  get isLive(): boolean {
    return this.isDesktop && !this.testView.isActive;
  }

  /** Reads Steam's status, retrying while a previous native check is still running. */
  async readStatus(): Promise<SteamStatus> {
    const deadline = Date.now() + BUSY_RETRY_MS;
    for (;;) {
      try {
        return await this.readStatusOnce();
      } catch (error) {
        if (DesktopError.from(error).kind !== "busy" || Date.now() >= deadline) throw error;
        await new Promise(resolve => setTimeout(resolve, BUSY_RETRY_INTERVAL_MS));
      }
    }
  }

  async chooseSteamExecutable(): Promise<SteamStatus | null> {
    this.requireDesktop("Open the desktop app to choose an executable. This browser preview cannot change your computer.");
    const path = await open({
      multiple: false,
      directory: false,
      title: "Locate steam.exe",
      filters: [{ name: "Windows application", extensions: ["exe"] }],
    });
    if (!path) return null;
    return SteamStatus.from(await this.call<SteamSnapshot>("set_steam_path", { path }));
  }

  async restartSteam(force: boolean, onLoadingUi?: () => void): Promise<void> {
    this.requireDesktop("Restart is available in the installed desktop app.");
    const progress = new Channel<string>();
    let finished = false;
    progress.onmessage = phase => {
      if (!finished && phase === "loading-ui") onLoadingUi?.();
    };
    try {
      await this.call<string>("restart_steam", { force, progress });
    } finally {
      finished = true;
    }
  }

  async setNetworkPaused(paused: boolean): Promise<SteamStatus> {
    this.requireDesktop("Network controls are available in the installed desktop app.");
    return SteamStatus.from(await this.call<SteamSnapshot>("set_network_paused", { paused }));
  }

  async findUpdate(): Promise<AvailableUpdate | null> {
    if (!this.isLive) return null;
    return (await this.call<{ update: AvailableUpdate | null }>("find_update")).update;
  }

  async installUpdate(): Promise<void> {
    this.requireDesktop("Updates are available in the desktop app.");
    await this.call("install_update");
  }

  /** Writes a timing to the desktop's `timings.log`; failures are ignored. */
  logTiming(entry: TimingEntry): void {
    if (this.isDesktop) void invoke("log_timing", { ...entry }).catch(() => undefined);
  }

  private async readStatusOnce(): Promise<SteamStatus> {
    if (!this.isLive) return SteamStatus.notDetected;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const timeout = new Promise<never>((_, reject) => {
      const message = `Steam status check timed out after ${STATUS_TIMEOUT_MS / 1000} seconds.`;
      timer = setTimeout(() => reject(new DesktopError("failed", message)), STATUS_TIMEOUT_MS);
    });
    try {
      return SteamStatus.from(await Promise.race([this.call<SteamSnapshot>("get_status"), timeout]));
    } finally {
      clearTimeout(timer);
    }
  }

  private requireDesktop(message: string): void {
    if (this.testView.isActive) throw new DesktopError("failed", "System actions are disabled in UI test view.");
    if (!this.isDesktop) throw new DesktopError("failed", message);
  }

  private async call<T>(command: string, args?: InvokeArgs): Promise<T> {
    try {
      return await this.timings.timeCommand(command, () => invoke<T>(command, args), command === "get_status");
    } catch (error) {
      throw DesktopError.from(error);
    }
  }
}

