import type { StatusIconState } from "../../app/views/SteamToolView";
import { IconAlert, IconSpinner, IconPlay, IconRestart, IconWifi, IconWifiOff } from "./PixelIcons";

const icons = {
  restarting: IconRestart,
  connecting: IconSpinner,
  running: IconPlay,
  offline: IconWifiOff,
  connected: IconWifi,
  unknown: IconAlert,
} as const;

/** The small pixel icon beside a status line; renders nothing without a state. */
export function StatusIcon({ state, label }: { state: StatusIconState; label: string }) {
  if (!state) return null;
  const Icon = icons[state];
  return (
    <span className={`status-icon ${state}`} role="img" aria-label={label} title={label}>
      <Icon size={17} className={state === "restarting" ? "restart-spin" : undefined} />
    </span>
  );
}
