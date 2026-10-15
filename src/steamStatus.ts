import type { Status } from "./api";

export function connectionLabel(status: Status): string {
  if (!status.steamPath) return "Steam location needed";
  if (!status.steamRunning) return "Steam is not running";
  if (status.blocked) return "Steam offline";
  if (status.restoringSince !== undefined) return "Restoring connection…";
  if (status.connectionPending && status.steamOnline === null) return "Connecting…";
  if (status.connectionStale && status.steamOnline !== null) {
    return status.steamOnline ? "Last confirmed online" : "Last confirmed offline";
  }
  if (status.steamOnline === true) return "Steam online";
  if (status.steamOnline === false) return "Steam offline";
  return "Steam connection unavailable";
}
