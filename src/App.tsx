import { useEffect, useRef, useState } from "react";
import {
  Check,
  ArrowLeft,
  CircleAlert,
  Power,
  ChevronDown,
  ChevronRight,
  FolderOpen,
  Grid2X2,
  LoaderCircle,
  Preview,
  Radio,
  RotateCw,
  Settings2,
  Wifi,
  WifiOff,
  X,
  SteamFriendLogo,
} from "./PixelIcons";
import appIcon from "../assets/app-logo.png";
import {
  desktop,
  getLocalUpdate,
  getStatus,
  installLocalUpdate,
  type LocalUpdate,
  pickExecutable,
  restartSteam,
  setBlocked,
  type Status,
} from "./api";

type Page = "library" | "steamy";
type LogEntry = { message: string; time: string };

const initialStatus: Status = {
  steamPath: null,
  steamRunning: false,
  targetPath: null,
  blocked: false,
  elevated: false,
};

function StatusIcon({ state, label }: { state: string; label: string }) {
  const Icon = state === "error" ? CircleAlert : state === "busy" ? LoaderCircle : state === "blocked" ? WifiOff : state === "online" ? Check : Power;
  return <span className={`status-icon ${state}`} role="img" aria-label={label} title={label}><Icon size={17} aria-hidden="true" /></span>;
}


export default function App() {
  const [page, setPage] = useState<Page>("library");
  const [status, setStatus] = useState(initialStatus);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [localUpdate, setLocalUpdate] = useState<LocalUpdate | null>(null);
  const [modal, setModalState] = useState<"settings" | "force" | null>(null);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const busyRef = useRef(false);
  const statusGeneration = useRef(0);
  const dialogRef = useRef<HTMLElement>(null);
  const returnFocusRef = useRef<HTMLElement | null>(null);

  const toolState = error
    ? { label: "Needs attention", className: "error" }
    : busy
      ? { label: "Working", className: "busy" }
      : status.blocked
        ? { label: "Steam network blocked", className: "blocked" }
        : status.steamRunning
          ? { label: "Steam running", className: "online" }
          : { label: "Ready", className: "idle" };

  function setModal(next: "settings" | "force" | null) {
    if (next && !modal) returnFocusRef.current = document.activeElement as HTMLElement | null;
    setModalState(next);
  }

  const addLog = (message: string) =>
    setLogs((previous) =>
      [
        {
          message,
          time: new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }),
        },
        ...previous,
      ].slice(0, 4),
    );

  useEffect(() => {
    let active = true;
    const refresh = async () => {
      if (busyRef.current) return;
      const generation = statusGeneration.current;
      try {
        const next = await getStatus();
        if (active && generation === statusGeneration.current && !busyRef.current) setStatus(next);
      } catch (err) {
        if (active) setError(String(err));
      } finally {
        if (active) setLoading(false);
      }
    };
    void refresh();
    const timer = window.setInterval(refresh, 3000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, []);

  useEffect(() => {
    let active = true;
    void getLocalUpdate()
      .then((result) => {
        if (!active) return;
        setLocalUpdate(result.update);
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, []);

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
      const message = String(err);
      if (message.includes("STEAM_STILL_RUNNING")) setModal("force");
      else {
        setError(message);
        addLog("Action could not be completed.");
      }
    } finally {
      busyRef.current = false;
      setBusy(null);
    }
  }

  function restart(force = false) {
    setModal(null);
    void action("restart", async () => {
      const message = await restartSteam(force);
      setStatus(await getStatus());
      setNotice(message);
      addLog(message);
    });
  }

  function toggleNetwork() {
    void action("network", async () => {
      const next = await setBlocked(!status.blocked);
      setStatus(next);
      const message = next.blocked
        ? "Steam network traffic is paused."
        : "Steam network access is restored.";
      setNotice(message);
      addLog(message);
    });
  }

  function checkForUpdate() {
    void action("check-update", async () => {
      const result = await getLocalUpdate();
      setLocalUpdate(result.update);
      const message = !result.sharedBuildsAvailable
        ? "Shared updates are not enabled for this copy."
        : result.update
          ? `Version ${result.update.version} is ready to install.`
          : "RBX Tools is up to date.";
      setNotice(message);
      addLog(message);
    });
  }

  function installSharedUpdate() {
    void action("install-update", async () => {
      await installLocalUpdate();
    });
  }

  function chooseSteam() {
    void action("path", async () => {
      const next = await pickExecutable("steam");
      if (next) {
        setStatus(next);
        addLog("Steam location updated.");
      }
    });
  }

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <button className="brand" onClick={() => setPage("library")} aria-label="RBX Tools home">
          <img src={appIcon} alt="" />
          <strong>RBX <span>TOOLS</span></strong>
        </button>

        <div className="nav-label">PLAYER MENU</div>
        <nav aria-label="Main navigation">
          <button
            className={page === "library" ? "nav-item active" : "nav-item"}
            onClick={() => setPage("library")}
          >
            <Grid2X2 size={17} /> Tool library
            <span className="nav-count">1</span>
          </button>
          <button
            className={page === "steamy" ? "nav-item active" : "nav-item"}
            onClick={() => setPage("steamy")}
          >
            <SteamFriendLogo small />
            <span className="nav-tool-name">Steamy Friends</span>
            <StatusIcon state={toolState.className} label={toolState.label} />
          </button>
        </nav>

        <div className="sidebar-bottom">
          <button className="nav-item" onClick={() => setModal("settings")}>
            <Settings2 size={17} /> Settings
          </button>
          <div className="sidebar-version">
            <span>RBX TOOLS</span>
            <span>v0.2.7</span>
          </div>
        </div>
      </aside>

      <div className="workspace">
        <div className="pocket-landscape" aria-hidden="true" />
        <header className="topbar">
          <nav className="breadcrumb" aria-label="Breadcrumb">
            <button onClick={() => setPage("library")}><ArrowLeft size={18} /> Tool library</button>
          </nav>
        </header>

        <main>
          {!desktop && (
            <div className="preview-strip">
              <Preview size={14} /> Interface preview · Desktop actions are available in the Windows app.
            </div>
          )}

          {error && (
            <div role="alert" className="message error">
              <span>{error}</span>
              <button aria-label="Dismiss error" onClick={() => setError(null)}><X size={16} /></button>
            </div>
          )}
          {notice && (
            <div role="status" className="message success">
              <Check size={16} />
              <span>{notice}</span>
              <button aria-label="Dismiss notification" onClick={() => setNotice(null)}><X size={16} /></button>
            </div>
          )}

          {page === "library" ? (
            <section className="library-page">
              <div className="page-heading">
                <div>
                  <h1>Your tools</h1>
                  <p>Small helpers for the games you play.</p>
                </div>
              </div>

              <div className="tools-grid">
                <button className="tool-card" onClick={() => setPage("steamy")}>
                  <div className="tool-art">
                    <div className="art-grid" />
                    <div className="friend-orbit"><SteamFriendLogo /></div>
                    <span className="art-status">
                      <StatusIcon state={toolState.className} label={toolState.label} />
                      {toolState.label}
                    </span>
                  </div>
                  <div className="tool-body">
                    <div className="tool-title">
                      <h2>Steamy Friends</h2>
                      <ChevronRight size={21} />
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
                <SteamFriendLogo />
                <h1>Steamy Friends</h1>
              </div>

              <div className="control-grid">
                <section className="control-card">
                  <div className="control-icon"><RotateCw size={24} /></div>
                  <h2>Restart Steam</h2>
                  <p>Close Steam, then bring it right back for a fresh session.</p>
                  <div className="control-status">
                    <StatusIcon state={status.steamRunning ? "online" : "idle"} label={status.steamRunning ? "Running" : "Inactive"} />
                    {loading
                      ? "Checking Steam…"
                      : status.steamRunning
                        ? "Steam is running"
                        : status.steamPath
                          ? "Steam is not running"
                          : "Steam location needed"}
                  </div>
                  <button
                    className="primary-button"
                    disabled={!!busy || loading || !desktop || !status.steamPath}
                    onClick={() => restart()}
                  >
                    {busy === "restart" ? <LoaderCircle size={16} /> : <RotateCw size={16} />}
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
                    <StatusIcon state={status.blocked ? "blocked" : status.targetPath ? "online" : "idle"} label={status.blocked ? "Blocked" : status.targetPath ? "Unblocked" : "Setup needed"} />
                    {status.blocked
                      ? "Incoming and outgoing traffic blocked"
                      : status.targetPath
                        ? "No RBX block active"
                        : "Steam location needed"}
                  </div>
                  <button
                    className={status.blocked ? "primary-button amber-button" : "secondary-button"}
                    disabled={!!busy || loading || !desktop || !status.targetPath}
                    onClick={toggleNetwork}
                  >
                    {busy === "network" ? (
                      <LoaderCircle size={16} />
                    ) : status.blocked ? (
                      <Wifi size={16} />
                    ) : (
                      <WifiOff size={16} />
                    )}
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

              <details className="activity-panel">
                <summary>
                  <span><Radio size={15} /> Session activity</span>
                  <ChevronDown size={16} />
                </summary>
                <div className="activity-content">
                  {logs.length ? (
                    logs.map((log, index) => (
                      <div className="log-row" key={`${log.time}-${index}`}>
                        <span className="log-bullet" />
                        <span>{log.message}</span>
                        <time>{log.time}</time>
                      </div>
                    ))
                  ) : (
                    <div className="activity-empty">Nothing to report yet.</div>
                  )}
                </div>
              </details>
            </section>
          )}
        </main>
      </div>

      {modal && (
        <div className="modal-backdrop" onClick={() => { if (modal !== "force") setModal(null); }}>
          <section
            ref={dialogRef}
            className="modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="modal-title"
            onClick={(event) => event.stopPropagation()}
            onKeyDown={(event) => { if (event.key === "Escape" && modal !== "force") setModal(null); }}
          >
            <button className="modal-close" aria-label="Close dialog" onClick={() => setModal(null)} autoFocus>
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
                <div className="setting-row">
                  <div>
                    <strong>{localUpdate ? `Update v${localUpdate.version} ready` : "Updates"}</strong>
                    {localUpdate && (
                      <p>{localUpdate.notes || "A newer shared build is ready."}</p>
                    )}
                  </div>
                  <button
                    className="small-button"
                    disabled={!!busy || !desktop}
                    onClick={localUpdate ? installSharedUpdate : checkForUpdate}
                  >
                    {busy === "install-update"
                      ? "Starting…"
                      : busy === "check-update"
                        ? "Checking…"
                        : localUpdate
                          ? "Install update"
                          : "Check for updates"}
                  </button>
                </div>
                <p className="modal-footnote">RBX Tools v0.2.7 · Windows desktop edition</p>
              </>
            ) : (
              <>
                <RotateCw className="accent" size={26} />
                <h2 id="modal-title">Steam is still running</h2>
                <p className="modal-intro">
                  Steam did not finish closing. Force-close it and restart? This can interrupt downloads and cloud sync.
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
