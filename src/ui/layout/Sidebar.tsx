import appIcon from "../../../assets/app-logo.png";
import type { AppController } from "../../app/AppController";
import type { Page } from "../../app/AppState";
import type { SteamToolView } from "../../app/views/SteamToolView";
import { IconLibrary, IconSettings, SteamFriendLogo } from "../components/PixelIcons";
import { StatusIcon } from "../components/StatusIcon";

type SidebarActions = Pick<AppController, "showPage" | "openModal">;

/** Brand, page navigation, Settings, and the version. */
export function Sidebar({ page, steamTool, appVersion, actions }: {
  page: Page;
  steamTool: SteamToolView;
  appVersion: string;
  actions: SidebarActions;
}) {
  const navClass = (target: Page) => page === target ? "nav-item active" : "nav-item";
  return (
    <aside className="sidebar">
      <div className="brand">
        <span className="brand-emblem"><img src={appIcon} alt="" /></span>
        <strong>RBX <span>TOOLS</span></strong>
      </div>

      <nav aria-label="Main navigation">
        <button className={navClass("library")} onClick={() => actions.showPage("library")}>
          <IconLibrary size={19} /> <span className="ui-label">Tool Library</span>
          <span className="nav-count"><span className="ui-label">1</span></span>
        </button>
        <button className={navClass("steamy")} onClick={() => actions.showPage("steamy")}>
          <SteamFriendLogo small />
          <span className="nav-tool-name ui-label">Steamy Friends</span>
          <StatusIcon state={steamTool.toolIcon} label={steamTool.toolLabel} />
        </button>
      </nav>

      <div className="sidebar-bottom">
        <button className="nav-item" onClick={() => actions.openModal("settings")}>
          <IconSettings size={19} /> <span className="ui-label">Settings</span>
        </button>
        <div className="sidebar-version">
          <span>RBX TOOLS</span>
          <span>v{appVersion}</span>
        </div>
      </div>
    </aside>
  );
}
