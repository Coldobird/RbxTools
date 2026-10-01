import type { Status } from "./api";

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
    && left.targetPath === right.targetPath
    && left.blocked === right.blocked
    && left.elevated === right.elevated;
}
