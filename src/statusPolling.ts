import type { Status } from "./api";

export const CONNECTION_GRACE_MS = 30_000;

// Reset on resume so time spent minimized never ages a retained reading.
export function createConnectionGrace(now = Date.now) {
  let lastResponse = now();
  return {
    reset() { lastResponse = now(); },
    observe(status?: Status) {
      if (status && (status.steamOnline !== null || !status.steamRunning || status.blocked)) lastResponse = now();
      return now() - lastResponse >= CONNECTION_GRACE_MS;
    },
  };
}

type PollOptions = {
  read: () => Promise<Status>;
  canPoll: () => boolean;
  generation: () => number;
  onStatus: (status: Status) => void;
  onError: (error: unknown) => void;
};

// Schedule after each request settles so slow native calls cannot accumulate.
export function startStatusPolling(options: PollOptions, interval = 3000) {
  let active = true;
  let pending = false;
  let timer: ReturnType<typeof setTimeout> | undefined;

  async function refresh() {
    if (!active || pending) return;
    clearTimeout(timer);
    if (!options.canPoll()) {
      timer = setTimeout(refresh, interval);
      return;
    }
    pending = true;
    const generation = options.generation();
    const current = () => active && generation === options.generation() && options.canPoll();
    try {
      const next = await options.read();
      if (current()) options.onStatus(next);
    } catch (error) {
      if (current()) options.onError(error);
    } finally {
      pending = false;
      if (active) timer = setTimeout(refresh, interval);
    }
  }

  void refresh();
  return {
    refresh,
    stop() {
      active = false;
      clearTimeout(timer);
    },
  };
}

export function sameStatus(left: Status, right: Status): boolean {
  return left.steamPath === right.steamPath
    && left.steamRunning === right.steamRunning
    && left.steamOnline === right.steamOnline
    && !!left.connectionStale === !!right.connectionStale
    && !!left.connectionPending === !!right.connectionPending
    && left.restoringSince === right.restoringSince
    && left.reconnectError === right.reconnectError
    && left.targetPath === right.targetPath
    && left.blocked === right.blocked
    && left.elevated === right.elevated;
}

export function mergeStatus(previous: Status, next: Status, expired = true, now = Date.now()): Status {
  const keepConnection = next.steamOnline === null
    && previous.steamOnline !== null
    && next.steamPath !== null
    && next.steamPath === previous.steamPath
    && previous.steamRunning && next.steamRunning
    && !previous.blocked && !next.blocked;
  const merged = {
    ...next,
    steamOnline: keepConnection ? previous.steamOnline : next.steamOnline,
    connectionStale: keepConnection && expired,
    connectionPending: next.steamOnline === null && !expired,
    restoringSince: previous.restoringSince !== undefined
      && now - previous.restoringSince < CONNECTION_GRACE_MS
      && next.steamRunning && !next.blocked && next.steamOnline !== true && !next.reconnectError
      ? previous.restoringSince : undefined,
  };
  return sameStatus(previous, merged) ? previous : merged;
}

// Each action owns its fields, so an older full snapshot cannot undo its peer.
export function mergeRestartResult(current: Status, result: Status): Status {
  return { ...result, blocked: current.blocked, reconnectError: current.reconnectError,
    steamOnline: current.blocked ? false : result.blocked ? null : result.steamOnline };
}

export function mergeNetworkResult(current: Status, result: Status, restartChanged: boolean): Status {
  return { ...current, blocked: result.blocked, reconnectError: result.reconnectError,
    steamOnline: result.blocked ? false : restartChanged || result.steamRunning !== current.steamRunning ? null : result.steamOnline,
    connectionStale: false };
}
