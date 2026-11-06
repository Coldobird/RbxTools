import type { SteamStatus } from "../../domain/SteamStatus";
import { StateView } from "./StateView";

/** Which pixel icon a status line shows, or none. */
export type StatusIconState = "restarting" | "connecting" | "running" | "offline" | "connected" | "unknown" | null;

const NO_STEAM = "Steam location needed";
const NOT_RUNNING = "Steam is not running";

/**
 * What the Steamy Friends tool shows: status lines, icons, button labels, and
 * which controls are available.
 */
export class SteamToolView extends StateView {
  // ── Busy states ──────────────────────────────────────────────────────────

  /** The restart action is running (Steam closing, starting, or its window loading). */
  get isRestartActionRunning(): boolean {
    return this.isRunning("restart");
  }

  get isChangingNetwork(): boolean {
    return this.isRunning("network");
  }

  get hasSteamPath(): boolean {
    return this.status.steamPath !== null;
  }

  // ── Restart control ──────────────────────────────────────────────────────

  get restartLabel(): string | null {
    switch (this.state.restartPhase) {
      case "restarting": return "Restarting…";
      case "loading-ui": return "Loading UI…";
      case "loaded": return this.status.steamRunning ? "Loaded" : null;
      default: return null;
    }
  }

  get restartIcon(): StatusIconState {
    switch (this.state.restartPhase) {
      case "restarting": return "restarting";
      case "loading-ui": return "connecting";
      case "loaded": return this.status.steamRunning ? "running" : null;
      default: return null;
    }
  }

  get restartStatusText(): string {
    if (this.state.loading) return "Checking Steam…";
    if (this.restartLabel) return this.restartLabel;
    if (this.status.steamRunning) return "Steam is running";
    return this.hasSteamPath ? NOT_RUNNING : NO_STEAM;
  }

  get restartButtonLabel(): string | null {
    if (this.isRestartActionRunning) return this.restartLabel;
    return this.status.steamRunning ? "Restart Steam" : "Start Steam";
  }

  get canRestart(): boolean {
    return !this.isRestartActionRunning && this.steamControlsAvailable;
  }

  // ── Network control ──────────────────────────────────────────────────────

  get isNetworkPaused(): boolean {
    return this.status.networkPaused;
  }

  get networkLabel(): string {
    if (this.isChangingNetwork && this.isNetworkPaused) return "Restoring connection…";
    if (this.state.loading) return "Connecting…";
    return this.connectionLabel;
  }

  get networkIcon(): StatusIconState {
    const { status } = this;
    if ((this.isChangingNetwork && status.networkPaused) || status.isRestoring || this.state.loading) return "connecting";
    if (status.networkPaused) return "offline";
    if (status.steamRunning && status.steamOnline === null) return "unknown";
    if (status.steamOnline === true) return "connected";
    if (status.steamRunning && status.steamOnline === false) return "offline";
    return null;
  }

  get networkHeading(): string {
    return this.isNetworkPaused ? "Network paused" : "Stop network";
  }

  get networkButtonLabel(): string {
    if (this.isChangingNetwork && this.isNetworkPaused) return "Restoring and reconnecting…";
    return this.isNetworkPaused ? "Restore and reconnect" : "Stop network";
  }

  get canToggleNetwork(): boolean {
    return !this.isChangingNetwork && this.steamControlsAvailable;
  }

  // ── Tool summary (sidebar and library card) ──────────────────────────────

  get toolIcon(): StatusIconState {
    return this.isRestartPhaseActive ? this.restartIcon : this.networkIcon ?? this.restartIcon;
  }

  get toolLabel(): string {
    const { error, running, loading } = this.state;
    if (error) return "Needs attention";
    if (this.isRestartPhaseActive) return this.restartLabel!;
    if (!running.isIdle || loading) return running.oldest === "network" ? "Changing network state" : "Checking Steam";
    if (this.status.networkPaused) return "Steam network paused";
    return this.connectionLabel;
  }

  // ── Internals ────────────────────────────────────────────────────────────

  private get status(): SteamStatus {
    return this.state.status;
  }

  /** Steam is known and no non-Steam action is in the way. */
  private get steamControlsAvailable(): boolean {
    return !this.state.running.hasNonSteamAction && !this.state.loading && this.hasSteamPath;
  }

  /** Steam is closing and starting, or its window is loading. */
  private get isRestartPhaseActive(): boolean {
    return this.state.restartPhase === "restarting" || this.state.restartPhase === "loading-ui";
  }

  /** Steam's connection, without claiming online just because Steam runs or a pause ended. */
  private get connectionLabel(): string {
    const { status } = this;
    if (!status.steamPath) return NO_STEAM;
    if (!status.steamRunning) return NOT_RUNNING;
    if (status.networkPaused) return "Steam offline";
    if (status.isRestoring) return "Restoring connection…";
    if (status.connectionPending && status.steamOnline === null) return "Connecting…";
    if (status.connectionStale && status.steamOnline !== null) {
      return status.steamOnline ? "Last confirmed online" : "Last confirmed offline";
    }
    if (status.steamOnline === true) return "Steam online";
    if (status.steamOnline === false) return "Steam offline";
    return "Steam connection unavailable";
  }
}
