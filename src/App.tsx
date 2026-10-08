import { useEffect, useRef, useState } from "react";
import {
  Check,
  ArrowLeft,
  CircleAlert,
  FolderOpen,
  Grid2X2,
  LoaderCircle,
  Play,
  Preview,
  RotateCw,
  Settings2,
  Wifi,
  WifiOff,
  X,
  SteamFriendLogo,
} from "./PixelIcons";
import appIcon from "../assets/app-logo.png";
import tauriConfig from "../src-tauri/tauri.conf.json";
import { sameStatus, startStatusPolling } from "./statusPolling";
import { connectionLabel } from "./steamStatus";
import {
  desktop,
  getUpdate,
  getStatus,
  installUpdate,
  makeSpacewarPrivate,
  type AvailableUpdate,
  pickExecutable,
  restartSteam,
  setBlocked,
  type Status,
} from "./api";

type Page = "library" | "steamy";
const appVersion = tauriConfig.version;

const initialStatus: Status = {
  steamPath: null,
  steamRunning: false,
  steamOnline: false,
  targetPath: null,
  blocked: false,
  elevated: false,
};
const previewSteamPath = "C:\\Preview\\Steam\\steam.exe";
const previewDelay = () => new Promise<void>((resolve) => window.setTimeout(resolve, 650));

type IconState = "restarting" | "connecting" | "running" | "blocked" | "connected" | null;

function StatusIcon({ state, label }: { state: IconState; label: string }) {
  if (!state) return null;
  const Icon = state === "connecting" ? LoaderCircle : state === "restarting" ? RotateCw : state === "running" ? Play : state === "blocked" ? WifiOff : Wifi;
  return <span className={`status-icon ${state}`} role="img" aria-label={label} title={label}><Icon size={17} className={state === "restarting" ? "restart-spin" : undefined} /></span>;
}


export default function App() {
  const [page, setPage] = useState<Page>("library");
  const [status, setStatus] = useState(initialStatus);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<{ message: string; update: boolean } | null>(null);
  const [availableUpdate, setAvailableUpdate] = useState<AvailableUpdate | null>(null);
  const [updateError, setUpdateError] = useState<string | null>(null);
  const [modal, setModalState] = useState<"settings" | "force" | "update" | null>(null);
  const [startupUpdatePending, setStartupUpdatePending] = useState(false);
  const [restartPhase, setRestartPhase] = useState<"restarting" | "running" | null>(null);
  const restartTimer = useRef<number | null>(null);
  const busyRef = useRef(false);
  const statusGeneration = useRef(0);
  const dialogRef = useRef<HTMLElement>(null);
  const returnFocusRef = useRef<HTMLElement | null>(null);

  useEffect(() => () => {
    if (restartTimer.current !== null) window.clearTimeout(restartTimer.current);
  }, []);

  const restartIcon: IconState = restartPhase === "restarting" ? "restarting" : restartPhase === "running" && status.steamRunning ? "running" : null;
  const networkIcon: IconState = status.blocked ? "blocked" : loading || status.steamRunning && status.steamOnline === null ? "connecting" : status.steamOnline === true ? "connected" : status.steamRunning && status.steamOnline === false ? "blocked" : null;
  const toolIcon: IconState = restartPhase === "restarting" ? "restarting" : networkIcon ?? restartIcon;

  const toolState = error
    ? { label: "Needs attention", className: "error" }
    : restartPhase === "restarting"
      ? { label: busy === "restart" ? "Restarting Steam" : "Confirming Steam startup", className: "busy" }
      : busy || loading
        ? { label: busy === "network" ? "Changing network state" : busy === "privacy" ? "Setting Spacewar privacy" : "Checking Steam", className: "busy" }
      : status.blocked
        ? { label: "Steam network blocked", className: "blocked" }
        : status.steamOnline === true
          ? { label: "Steam online", className: "connected" }
        : status.steamRunning
          ? { label: connectionLabel(status), className: status.steamOnline === null ? "busy" : "idle" }
          : status.steamPath
            ? { label: "Steam not running", className: "idle" }
            : { label: "Steam location needed", className: "setup" };

  function setModal(next: "settings" | "force" | "update" | null) {
    if (next && !modal) returnFocusRef.current = document.activeElement as HTMLElement | null;
    setModalState(next);
  }

  function showNotice(message: string, update = false) {
    setNotice({ message, update });
  }

  useEffect(() => {
    if (!desktop) {
      setLoading(false);
      return;
    }
    const poller = startStatusPolling({
      read: getStatus,
      canPoll: () => !busyRef.current && !document.hidden,
      generation: () => statusGeneration.current,
      onStatus(next) {
        setStatus((current) => sameStatus(current, next) ? current : next);
        setLoading(false);
      },
      onError(err) {
        setStatus((current) => ({ ...current, steamOnline: null }));
        setError(String(err));
        setLoading(false);
      },
    });
    const onVisibilityChange = () => {
      if (!document.hidden) void poller.refresh();
    };
    document.addEventListener("visibilitychange", onVisibilityChange);
    return () => {
      poller.stop();
      document.removeEventListener("visibilitychange", onVisibilityChange);
    };
  }, []);

  useEffect(() => {
    if (!error) return;
    const timer = window.setTimeout(() => setError(null), 8000);
    return () => window.clearTimeout(timer);
  }, [error]);

  useEffect(() => {
    if (status.reconnectError) setError(status.reconnectError);
  }, [status.reconnectError]);

  useEffect(() => {
    if (!notice || notice.update) return;
    const timer = window.setTimeout(() => setNotice(null), 5000);
    return () => window.clearTimeout(timer);
  }, [notice]);

  useEffect(() => {
    if (!desktop) return;
    let active = true;
    const check = (startup: boolean) => {
      void getUpdate()
        .then((result) => {
          if (!active) return;
          setAvailableUpdate(result.update);
          setUpdateError(null);
          if (result.update) {
            if (startup) setStartupUpdatePending(true);
            else showNotice(`RBX Tools v${result.update.version} is available.`, true);
          }
        })
        .catch((err) => {
          if (active) setUpdateError(String(err));
        });
    };
    check(true);
    const timer = window.setInterval(() => check(false), 6 * 60 * 60 * 1000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, []);

  useEffect(() => {
    if (!startupUpdatePending || modal || busy) return;
    setStartupUpdatePending(false);
    if (availableUpdate) {
      returnFocusRef.current = document.activeElement as HTMLElement | null;
      setModalState("update");
    }
  }, [startupUpdatePending, availableUpdate, modal, busy]);

  useEffect(() => {
    if (!modal) return;
    const dialog = dialogRef.current;
    const trapFocus = (event: KeyboardEvent) => {
      if (event.key !== "Tab" || !dialog) return;
      const controls = [...dialog.querySelectorAll<HTMLElement>('button:not(:disabled), a[href], input:not(:disabled), [tabindex="0"]')];
      const first = controls[0];
      const last = controls[controls.length - 1];
      if (event.shiftKey && (document.activeElement === first || !dialog.contains(document.activeElement))) {
        event.preventDefault();
        last?.focus();
      } else if (!event.shiftKey && (document.activeElement === last || !dialog.contains(document.activeElement))) {
        event.preventDefault();
        first?.focus();
      }
    };
    document.addEventListener("keydown", trapFocus);
    return () => {
      document.removeEventListener("keydown", trapFocus);
      returnFocusRef.current?.focus();
    };
  }, [modal]);

  async function action(name: string, work: () => Promise<void>) {
    if (busyRef.current) return;
    busyRef.current = true;
    statusGeneration.current += 1;
    setBusy(name);
    setError(null);
    setNotice(null);
    try {
      await work();
    } catch (err) {
      if (name === "restart") setRestartPhase(null);
      const message = String(err);
      if (message.includes("STEAM_STILL_RUNNING")) setModal("force");
      else {
        setError(message);
      }
    } finally {
      busyRef.current = false;
      setBusy(null);
    }
  }

  function restart(force = false) {
    if (busyRef.current) return;
    setModal(null);
    if (restartTimer.current !== null) window.clearTimeout(restartTimer.current);
    setRestartPhase("restarting");
    void action("restart", async () => {
      if (!desktop) {
        await previewDelay();
        setStatus((current) => ({ ...current, steamRunning: true, steamOnline: true, blocked: false }));
        showNotice("Steam restarted (preview only). No process was changed.");
        setRestartPhase("running");
        restartTimer.current = window.setTimeout(() => setRestartPhase(null), 5000);
        return;
      }
      const message = await restartSteam(force);
      setStatus(await getStatus());
      showNotice(message);
      setRestartPhase("running");
      restartTimer.current = window.setTimeout(() => setRestartPhase(null), 5000);
    });
  }

  function toggleNetwork() {
    if (busyRef.current) return;
    void action("network", async () => {
      if (!desktop) {
        await previewDelay();
        const blocked = !status.blocked;
        setStatus((current) => ({ ...current, blocked, steamOnline: !blocked && current.steamRunning }));
        showNotice(blocked
          ? "Steam network paused (preview only). No traffic was changed."
          : "Steam network restored (preview only). No traffic was changed.");
        return;
      }
      const next = await setBlocked(!status.blocked);
      setStatus(next);
      const message = next.blocked
        ? "Steam network traffic is paused."
        : "Steam network access is restored.";
      showNotice(message);
    });
  }

  function openPrivacy() {
    void action("privacy", async () => {
      if (!desktop) {
        await previewDelay();
        showNotice("Preview only. The desktop app signs in to Steam and automatically sets and verifies Spacewar privacy. No account was changed.");
        return;
      }
      showNotice(await makeSpacewarPrivate());
    });
  }

  function checkForUpdate() {
    void action("check-update", async () => {
      if (!desktop) {
        await previewDelay();
        showNotice("Update check complete (preview only). No update was installed.");
        return;
      }
      try {
        const result = await getUpdate();
        setAvailableUpdate(result.update);
        setUpdateError(null);
        showNotice(result.update
          ? `Version ${result.update.version} is ready to install.`
          : "RBX Tools is up to date.", !!result.update);
      } catch (err) {
        setUpdateError(String(err));
        throw err;
      }
    });
  }

  function installGithubUpdate() {
    void action("install-update", async () => {
      try {
        await installUpdate();
      } catch (err) {
        setUpdateError(String(err));
        throw err;
      }
    });
  }

  function chooseSteam() {
    void action("path", async () => {
      if (!desktop) {
        await previewDelay();
        setStatus((current) => ({ ...current, steamPath: previewSteamPath, targetPath: previewSteamPath }));
        showNotice("Demo steam.exe selected (preview only). No file was accessed.");
        return;
      }
      const next = await pickExecutable("steam");
      if (next) {
        setStatus(next);
      showNotice("Steam location updated.");
      }
    });
  }

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-emblem"><img src={appIcon} alt="" /></span>
          <strong>RBX <span>TOOLS</span></strong>
        </div>

        <nav aria-label="Main navigation">
          <button
            className={page === "library" ? "nav-item active" : "nav-item"}
            onClick={() => setPage("library")}
          >
            <Grid2X2 size={17} /> Tool Library
            <span className="nav-count">1</span>
          </button>
          <button
            className={page === "steamy" ? "nav-item active" : "nav-item"}
            onClick={() => setPage("steamy")}
          >
            <SteamFriendLogo small />
            <span className="nav-tool-name">Steamy Friends</span>
            <StatusIcon state={toolIcon} label={toolState.label} />
          </button>
        </nav>

        <div className="sidebar-bottom">
          <button className="nav-item" onClick={() => setModal("settings")}>
            <Settings2 size={17} /> Settings
          </button>
          <div className="sidebar-version">
            <span>RBX TOOLS</span>
            <span>v{appVersion}</span>
          </div>
        </div>
      </aside>

      <div className="workspace">
        <div className="pocket-landscape" aria-hidden="true" />
        <main>
          {page === "library" ? (
            <section className="library-page">
              <div className="page-heading">
                <div>
                  <h1>Tool Library</h1>
                </div>
              </div>

              <div className="tools-grid">
                <button className="tool-card" onClick={() => setPage("steamy")}>
                  <div className="tool-art">
                    <div className="art-grid" />
                    <div className="friend-orbit"><SteamFriendLogo /></div>
                    <span className="art-status">
                      <StatusIcon state={toolIcon} label={toolState.label} />
                      {toolState.label}
                    </span>
                  </div>
                  <div className="tool-body">
                    <div className="tool-title">
                      <h2>Steamy Friends</h2>
                    </div>
                    <p>Restart Steam or briefly pause its network connection.</p>
                    <div className="tool-tags">
                      <span><RotateCw size={12} /> Restart Steam</span>
                      <span><Wifi size={12} /> Network control</span>
                    </div>
                  </div>
                </button>

                <div className="empty-slot" aria-hidden="true">
                  <div className="slot-icon">+</div>
                  <h2>Room for more.</h2>
                  <p>The next useful little tool belongs here.</p>
                </div>
              </div>
            </section>
          ) : (
            <section className="tool-page">
              <div className="page-heading tool-page-heading">
                <button className="tool-back" onClick={() => setPage("library")} aria-label="Back to tool library" title="Back to tool library">
                  <ArrowLeft size={41} />
                </button>
                <SteamFriendLogo />
                <h1>Steamy Friends</h1>
              </div>

              <div className="control-grid">
                <section className="control-card">
                  <div className="control-icon"><RotateCw size={24} /></div>
                  <h2>Restart Steam</h2>
                  <p>Close Steam, then bring it right back for a fresh session.</p>
                  <div className="control-status">
                    <StatusIcon state={restartIcon} label={restartPhase === "restarting" ? "Restarting Steam" : "Steam running"} />
                    {loading
                      ? "Checking Steam…"
                      : busy === "restart"
                        ? "Restarting Steam…"
                      : restartPhase === "restarting"
                        ? "Confirming Steam startup…"
                      : status.steamRunning
                        ? "Steam is running"
                        : status.steamPath
                          ? "Steam is not running"
                          : "Steam location needed"}
                  </div>
                  <button
                    className="primary-button"
                    disabled={!!busy || loading || !status.steamPath}
                    onClick={() => restart()}
                  >
                    {busy === "restart" ? "Restarting Steam…" : "Restart Steam"}
                  </button>
                  <button className="text-button" disabled={!!busy} onClick={chooseSteam}>
                    <FolderOpen size={13} />
                    {status.steamPath ? "Change Steam location" : "Locate steam.exe"}
                  </button>
                </section>

                <section className={`control-card network-card ${status.blocked ? "is-blocked" : ""}`}>
                  <div className="control-icon">
                    {status.blocked ? <WifiOff size={24} /> : <Wifi size={24} />}
                  </div>
                  <h2>{status.blocked ? "Network paused" : "Stop network"}</h2>
                  <p>Pause incoming and outgoing traffic for Steam only.</p>
                  <div className="control-status">
                    <StatusIcon state={networkIcon} label={loading ? "Connecting…" : connectionLabel(status)} />
                    {busy === "network"
                      ? status.blocked ? "Restoring connection…" : "Pausing connection…"
                      : loading ? "Connecting…" : connectionLabel(status)}
                  </div>
                  <button
                    className={status.blocked ? "primary-button amber-button" : "secondary-button"}
                    disabled={!!busy || loading || !status.targetPath}
                    onClick={toggleNetwork}
                  >
                    {busy === "network"
                      ? status.blocked ? "Restoring and reconnecting…" : "Pausing connection…"
                      : status.blocked ? "Restore and reconnect" : "Stop network"}
                  </button>
                  {!status.targetPath && (
                    <button className="text-button" disabled={!!busy} onClick={chooseSteam}>
                      <FolderOpen size={13} /> Locate steam.exe
                    </button>
                  )}
                </section>
              </div>

            </section>
          )}
        </main>
      </div>

      {(error || notice) && (
        <div className="toast-stack">
          {error && (
            <div role="alert" className="toast error">
              {desktop ? <CircleAlert size={17} /> : <Preview size={17} />}
              <span>{error}</span>
              <button aria-label="Dismiss error" onClick={() => setError(null)}><X size={16} /></button>
            </div>
          )}
          {notice && (
            <div role="status" className="toast success">
              <Check size={17} />
              <span>{notice.message}</span>
              {notice.update && availableUpdate && (
                <button className="small-button toast-update" disabled={!!busy} onClick={installGithubUpdate}>Update</button>
              )}
              <button aria-label="Dismiss notification" onClick={() => setNotice(null)}><X size={16} /></button>
            </div>
          )}
        </div>
      )}

      {modal && (
        <div className="modal-backdrop" onClick={() => { if (modal !== "force" && busy !== "install-update") setModal(null); }}>
          <section
            ref={dialogRef}
            className="modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="modal-title"
            onClick={(event) => event.stopPropagation()}
            onKeyDown={(event) => { if (event.key === "Escape" && modal !== "force" && busy !== "install-update") setModal(null); }}
          >
            <button className="modal-close" aria-label="Close dialog" disabled={busy === "install-update"} onClick={() => setModal(null)} autoFocus>
              <X size={19} />
            </button>

            {modal === "settings" ? (
              <>
                <Settings2 className="accent" size={25} />
                <h2 id="modal-title">Settings</h2>
                <div className="setting-row">
                  <div>
                    <strong>Steam location</strong>
                    <p className="path-label">{status.steamPath ?? "Not detected yet"}</p>
                  </div>
                  <button className="small-button" disabled={!!busy} onClick={chooseSteam}>Browse</button>
                </div>
                <div className="setting-row" role="group" aria-labelledby="spacewar-privacy-title">
                  <div>
                    <strong id="spacewar-privacy-title">Spacewar privacy</strong>
                    <p>
                      {status.blocked
                        ? "Restore Steam’s network access first."
                        : desktop && !status.steamRunning
                          ? "Open Steam and sign in first."
                          : "Automatically hide its status from friends."}
                    </p>
                  </div>
                  <button
                    className="small-button"
                    disabled={!!busy || loading || status.blocked || (desktop && !status.steamRunning)}
                    onClick={openPrivacy}
                  >
                    {busy === "privacy" ? "Waiting for Steam…" : "Make Spacewar private"}
                  </button>
                </div>
                <div className="setting-row">
                  <div>
                    <strong>{availableUpdate ? `Update v${availableUpdate.version} ready` : "Updates"}</strong>
                    <p>{updateError ?? (availableUpdate
                      ? "Download from GitHub and restart to finish updating."
                      : "Checks GitHub automatically when you open the app.")}</p>
                  </div>
                  <button
                    className="small-button"
                    disabled={!!busy}
                    onClick={availableUpdate ? installGithubUpdate : checkForUpdate}
                  >
                    {busy === "install-update"
                      ? "Downloading…"
                      : busy === "check-update"
                        ? "Checking…"
                        : availableUpdate
                          ? "Update and restart"
                          : "Check for updates"}
                  </button>
                </div>
                <p className="modal-footnote">RBX Tools v{appVersion} · {desktop ? "Windows desktop edition" : "Browser preview · no system changes"}</p>
              </>
            ) : modal === "update" ? (
              <>
                <RotateCw className="accent" size={26} />
                <h2 id="modal-title">Update available</h2>
                <p className="modal-intro">
                  RBX Tools v{availableUpdate?.version} is ready. Update from GitHub and restart the app to finish installing.
                </p>
                {updateError && <p role="alert" className="modal-intro">{updateError}</p>}
                <div className="modal-actions">
                  <button className="secondary-button" disabled={busy === "install-update"} onClick={() => setModal(null)}>Cancel</button>
                  <button className="primary-button" disabled={!!busy || !availableUpdate} onClick={installGithubUpdate}>
                    {busy === "install-update" ? "Downloading…" : "Update"}
                  </button>
                </div>
              </>
            ) : (
              <>
                <RotateCw className="accent" size={26} />
                <h2 id="modal-title">Force restart Steam?</h2>
                <p className="modal-intro">
                  Force-close Steam and restart it with network access restored? This can interrupt downloads and cloud sync.
                </p>
                <div className="modal-actions">
                  <button className="secondary-button" onClick={() => setModal(null)}>Cancel</button>
                  <button className="primary-button" onClick={() => restart(true)}>Force restart</button>
                </div>
              </>
            )}
          </section>
        </div>
      )}
    </div>
  );
}
