import type { Status } from "./api";

export function connectionLabel(status: Status): string {
  if (status.blocked) return "Incoming and outgoing traffic blocked";
  if (!status.steamPath) return "Steam location needed";
  if (!status.steamRunning) return "Steam is not running";
  if (status.steamOnline === true) return "Steam online";
  if (status.steamOnline === false) return "Steam offline";
  return "Steam connection unavailable";
}
