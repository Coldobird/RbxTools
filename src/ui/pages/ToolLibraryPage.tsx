import type { AppController } from "../../app/AppController";
import type { SteamToolView } from "../../app/views/SteamToolView";
import { IconPlus, IconRestart, IconWifi, SteamFriendLogo } from "../components/PixelIcons";
import { StatusIcon } from "../components/StatusIcon";

/** The tool cards, plus a slot for the next tool. */
export function ToolLibraryPage({ steamTool, actions }: { steamTool: SteamToolView; actions: Pick<AppController, "showPage"> }) {
  return (
    <section className="library-page">
      <div className="page-heading">
        <h1>Tool Library</h1>
      </div>

      <div className="tools-grid">
        <button className="tool-card" onClick={() => actions.showPage("steamy")}>
          <div className="tool-art">
            <div className="art-grid" />
            <div className="friend-orbit"><SteamFriendLogo /></div>
            <span className="art-status">
              <StatusIcon state={steamTool.toolIcon} label={steamTool.toolLabel} />
              <span className="ui-label">{steamTool.toolLabel}</span>
            </span>
          </div>
          <div className="tool-body">
            <div className="tool-title">
              <h2>Steamy Friends</h2>
            </div>
            <p>Restart Steam or briefly pause its network connection.</p>
            <div className="tool-tags">
              <span><IconRestart size={13} /> <span className="ui-label">Restart Steam</span></span>
              <span><IconWifi size={13} /> <span className="ui-label">Network control</span></span>
            </div>
          </div>
        </button>

        <div className="empty-slot" aria-hidden="true">
          <div className="slot-icon"><IconPlus size={24} /></div>
          <h2>Room for more.</h2>
          <p>The next useful little tool belongs here.</p>
        </div>
      </div>
    </section>
  );
}
