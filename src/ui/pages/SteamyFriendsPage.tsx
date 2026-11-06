import type { AppController } from "../../app/AppController";
import type { SteamToolView } from "../../app/views/SteamToolView";
import { IconBack, IconFolder, IconRestart, IconWifi, IconWifiOff, SteamFriendLogo } from "../components/PixelIcons";
import { StatusIcon } from "../components/StatusIcon";

type SteamActions = Pick<AppController, "showPage" | "restartSteam" | "toggleNetwork" | "chooseSteamExecutable">;
type SteamToolProps = { steamTool: SteamToolView; actions: SteamActions };

/** The Steam tool: restart Steam, and pause and resume its network. */
export function SteamyFriendsPage({ steamTool, actions }: SteamToolProps) {
  return (
    <section className="tool-page">
      <div className="page-heading tool-page-heading">
        <button className="tool-back" onClick={() => actions.showPage("library")} aria-label="Back to tool library" title="Back to tool library">
          <IconBack size={41} />
        </button>
        <SteamFriendLogo />
        <h1>Steamy Friends</h1>
      </div>

      <div className="control-grid">
        <RestartCard steamTool={steamTool} actions={actions} />
        <NetworkCard steamTool={steamTool} actions={actions} />
      </div>
    </section>
  );
}

function RestartCard({ steamTool, actions }: SteamToolProps) {
  return (
    <section className="control-card">
      <div className="control-icon"><IconRestart size={24} /></div>
      <h2>Restart Steam</h2>
      <p>Close Steam, then bring it right back for a fresh session.</p>
      <div className="control-status">
        <StatusIcon state={steamTool.restartIcon} label={steamTool.restartLabel ?? "Steam running"} />
        <span className="ui-label">{steamTool.restartStatusText}</span>
      </div>
      <button className="primary-button" disabled={!steamTool.canRestart} onClick={() => actions.restartSteam()}>
        <span className="ui-label">{steamTool.restartButtonLabel}</span>
      </button>
      <button className="text-button" disabled={steamTool.isBusy} onClick={() => actions.chooseSteamExecutable()}>
        <IconFolder size={15} />
        <span className="ui-label">{steamTool.hasSteamPath ? "Change Steam location" : "Locate steam.exe"}</span>
      </button>
    </section>
  );
}

function NetworkCard({ steamTool, actions }: SteamToolProps) {
  const paused = steamTool.isNetworkPaused;
  return (
    <section className={`control-card network-card ${paused ? "is-paused" : ""}`}>
      <div className="control-icon">
        {paused ? <IconWifiOff size={24} /> : <IconWifi size={24} />}
      </div>
      <h2>{steamTool.networkHeading}</h2>
      <p>Pause incoming and outgoing traffic for Steam only.</p>
      <div className="control-status">
        <StatusIcon state={steamTool.networkIcon} label={steamTool.networkLabel} />
        <span className="ui-label">{steamTool.networkLabel}</span>
      </div>
      <button
        className={paused ? "primary-button amber-button" : "secondary-button"}
        disabled={!steamTool.canToggleNetwork}
        onClick={() => actions.toggleNetwork()}
      >
        <span className="ui-label">{steamTool.networkButtonLabel}</span>
      </button>
      {!steamTool.hasSteamPath && (
        <button className="text-button" disabled={steamTool.isBusy} onClick={() => actions.chooseSteamExecutable()}>
          <IconFolder size={15} /> <span className="ui-label">Locate steam.exe</span>
        </button>
      )}
    </section>
  );
}
