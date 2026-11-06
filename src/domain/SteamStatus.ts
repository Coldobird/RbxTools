/** How long a missing reading may keep showing the last confirmed connection. */
export const CONNECTION_GRACE_MS = 30_000;

/** What the desktop backend reports about the Steam client. */
export interface SteamSnapshot {
  readonly steamPath: string | null;
  readonly steamRunning: boolean;
  readonly steamOnline: boolean | null;
  /** RBX Tools has paused Steam's network by taking it offline. */
  readonly networkPaused: boolean;
}

/** Frontend memory layered over backend snapshots between polls. */
interface ConnectionMemory {
  /** The connection value was retained after the grace period expired. */
  readonly connectionStale?: boolean;
  /** No reading yet, but the grace period has not expired. */
  readonly connectionPending?: boolean;
  /** When a restore started; cleared once Steam confirms or the grace period ends. */
  readonly restoringSince?: number;
}

type StatusFields = SteamSnapshot & ConnectionMemory;

/**
 * An immutable reading of the Steam client, with the memory the interface keeps
 * between readings.
 *
 * Merges keep the connection label stable across slow or missing readings,
 * and keep each concurrent action's fields from being undone by another.
 */
export class SteamStatus implements StatusFields {
  /** No Steam found yet: the state before the first reading. */
  static readonly notDetected = new SteamStatus({ steamPath: null, steamRunning: false, steamOnline: false, networkPaused: false });

  readonly steamPath: string | null;
  readonly steamRunning: boolean;
  readonly steamOnline: boolean | null;
  readonly networkPaused: boolean;
  readonly connectionStale?: boolean;
  readonly connectionPending?: boolean;
  readonly restoringSince?: number;

  private constructor(fields: StatusFields) {
    this.steamPath = fields.steamPath;
    this.steamRunning = fields.steamRunning;
    this.steamOnline = fields.steamOnline;
    this.networkPaused = fields.networkPaused;
    if (fields.connectionStale !== undefined) this.connectionStale = fields.connectionStale;
    if (fields.connectionPending !== undefined) this.connectionPending = fields.connectionPending;
    if (fields.restoringSince !== undefined) this.restoringSince = fields.restoringSince;
    Object.freeze(this);
  }

  /** A fresh backend reading, without any frontend memory. */
  static from(snapshot: SteamSnapshot): SteamStatus {
    const { steamPath, steamRunning, steamOnline, networkPaused } = snapshot;
    return new SteamStatus({ steamPath, steamRunning, steamOnline, networkPaused });
  }

  with(changes: Partial<StatusFields>): SteamStatus {
    return new SteamStatus({ ...this.fields(), ...changes });
  }

  equals(other: SteamStatus): boolean {
    return this.steamPath === other.steamPath
      && this.steamRunning === other.steamRunning
      && this.steamOnline === other.steamOnline
      && !!this.connectionStale === !!other.connectionStale
      && !!this.connectionPending === !!other.connectionPending
      && this.restoringSince === other.restoringSince
      && this.networkPaused === other.networkPaused;
  }

  get isRestoring(): boolean {
    return this.restoringSince !== undefined;
  }

  /** Steam runs with its network resumed, but has not confirmed it is online. */
  get awaitsConnection(): boolean {
    return this.steamRunning && !this.networkPaused && this.steamOnline !== true;
  }

  /**
   * Applies a polled reading. A missing connection value keeps the previous one
   * for the same running session whose network is not paused; it is marked
   * stale once the grace period expires. Returns `this` when nothing visible changed.
   */
  mergePolled(next: SteamStatus, graceExpired = true, now = Date.now()): SteamStatus {
    const keepConnection = next.steamOnline === null
      && this.steamOnline !== null
      && next.steamPath !== null
      && next.steamPath === this.steamPath
      && this.steamRunning && next.steamRunning
      && !this.networkPaused && !next.networkPaused;
    const stillRestoring = this.restoringSince !== undefined
      && now - this.restoringSince < CONNECTION_GRACE_MS
      && next.awaitsConnection;
    const merged = next.with({
      steamOnline: keepConnection ? this.steamOnline : next.steamOnline,
      connectionStale: keepConnection && graceExpired,
      connectionPending: next.steamOnline === null && !graceExpired,
      restoringSince: stillRestoring ? this.restoringSince : undefined,
    });
    return this.equals(merged) ? this : merged;
  }

  /** Applies a restart result without undoing a newer network pause or restore. */
  mergeRestartResult(result: SteamStatus): SteamStatus {
    return result.with({
      networkPaused: this.networkPaused,
      steamOnline: this.networkPaused ? false : result.networkPaused ? null : result.steamOnline,
    });
  }

  /**
   * Applies a network result. Its connection reading is discarded when Steam
   * restarted meanwhile, because it describes the previous session. A resume
   * started at `resumedAt` shows as restoring until Steam confirms it is online.
   */
  mergeNetworkResult(result: SteamStatus, restartedMeanwhile: boolean, resumedAt?: number): SteamStatus {
    const sessionChanged = restartedMeanwhile || result.steamRunning !== this.steamRunning;
    return this.with({
      networkPaused: result.networkPaused,
      steamOnline: result.networkPaused ? false : sessionChanged ? null : result.steamOnline,
      connectionStale: false,
      restoringSince: result.awaitsConnection ? resumedAt : undefined,
    });
  }

  private fields(): StatusFields {
    return { ...this };
  }
}
