import { useEffect, useRef, useState } from "react";
import {
  ArrowDownLeft,
  ArrowLeft,
  ArrowRight,
  ArrowUpRight,
  Check,
  ChevronRight,
  CircleHelp,
  FolderOpen,
  Gamepad2,
  Grid2X2,
  LoaderCircle,
  LockKeyhole,
  Radio,
  RotateCw,
  Settings2,
  ShieldCheck,
  Volume2,
  Wifi,
  WifiOff,
  X,
} from "lucide-react";
import {
  desktop,
  fileName,
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

function PixelFriend({ small = false }: { small?: boolean }) {
  return (
    <svg
      className={small ? "pixel-friend small" : "pixel-friend"}
      viewBox="0 0 24 24"
      aria-hidden="true"
      shapeRendering="crispEdges"
    >
      <path fill="currentColor" d="M5 4h14v2h2v12h-2v2H5v-2H3V6h2z" />
      <path fill="#182421" d="M7 9h3v4H7zm7 0h3v4h-3zM8 16h8v2H8z" />
      <path fill="#e6fbbd" d="M5 6h14v2H5z" />
      <path fill="#182421" d="M0 12h3v5H0zm21 0h3v5h-3z" />
      <path fill="currentColor" d="M6 20h4v3H6zm8 0h4v3h-4z" />
    </svg>
  );
}

function Landscape() {
  return (
    <svg
      className="landscape"
      viewBox="0 0 440 230"
      aria-hidden="true"
      preserveAspectRatio="xMidYMid slice"
      shapeRendering="crispEdges"
    >
      <defs>
        <pattern
          id="stars"
          width="85"
          height="63"
          patternUnits="userSpaceOnUse"
        >
          <path d="M14 17h3v3h-3zM63 43h2v2h-2z" fill="#a95d64" opacity=".45" />
        </pattern>
      </defs>
      <rect width="440" height="230" fill="url(#stars)" />
      <path d="M325 40h36v8h8v31h-8v8h-36v-8h-8V48h8z" fill="#ff7b84" />
      <path d="M340 40h21v8h8v31h-8v8h-21v-8h-9V48h9z" fill="#ffadb2" />
      <path
        d="M0 160h25v-25h30v-22h25V96h40v25h22v20h29v-19h25v-28h28V76h24v27h25v30h37v25h38v-15h32v-24h35v20h25v91H0z"
        fill="#421f27"
      />
      <path
        d="M0 188h35v-18h40v-16h34v22h32v14h33v-21h30v-16h31v19h40v19h41v-28h37v-20h30v27h26v17h31v43H0z"
        fill="#5d2931"
      />
      <path
        d="M0 213h61v-10h43v9h60v-11h63v12h52v-16h55v9h45v-9h61v33H0z"
        fill="#9b3f49"
      />
      <path d="M0 225h95v-7h71v8h91v-8h77v7h106v5H0z" fill="#da5964" />
      <path
        d="M74 142h6v61h-6zM54 160h47v7H54zm9-12h28v9H63zm4-12h20v9H67zm5-9h10v9H72zM385 160h5v46h-5zm-14 16h32v7h-32zm7-12h19v9h-19zm5-10h9v9h-9z"
        fill="#32171c"
      />
    </svg>
  );
}

export default function App() {
  const [page, setPage] = useState<Page>("library");
  const [status, setStatus] = useState(initialStatus);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [localUpdate, setLocalUpdate] = useState<LocalUpdate | null>(null);
  const [modal, setModalState] = useState<"settings" | "help" | "force" | null>(
    null,
  );
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const busyRef = useRef(false);
  const statusGeneration = useRef(0);
  const dialogRef = useRef<HTMLElement>(null);
  const returnFocusRef = useRef<HTMLElement | null>(null);
  function setModal(next: "settings" | "help" | "force" | null) {
    if (next && !modal) returnFocusRef.current = document.activeElement as HTMLElement | null;
    setModalState(next);
  }
  const addLog = (message: string) =>
    setLogs((previous) =>
      [
        {
          message,
          time: new Date().toLocaleTimeString([], {
            hour: "2-digit",
            minute: "2-digit",
          }),
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
      .then((update) => {
        if (active) setLocalUpdate(update);
      })
      // Friends without the shared OneDrive folder use the GitHub update channel.
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
        event.preventDefault(); last?.focus();
      } else if (!event.shiftKey && (document.activeElement === last || !dialog.contains(document.activeElement))) {
        event.preventDefault(); first?.focus();
      }
    };
    document.addEventListener("keydown", trapFocus);
    return () => { document.removeEventListener("keydown", trapFocus); returnFocusRef.current?.focus(); };
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
        ? `Network paused for ${fileName(next.targetPath)}.`
        : "Network restored. Reconnect assist will run if Steam stays offline after a long pause.";
      setNotice(message);
      addLog(message);
    });
  }

  function checkForSharedUpdate() {
    void action("check-update", async () => {
      const update = await getLocalUpdate();
      setLocalUpdate(update);
      const message = update
        ? `Version ${update.version} is ready from your shared Builds folder.`
        : "You already have the newest shared build.";
      setNotice(message);
      addLog(message);
    });
  }

  function installSharedUpdate() {
    void action("install-update", async () => {
      await installLocalUpdate();
    });
  }

  function choose(kind: "steam" | "target") {
    void action("path", async () => {
      const next = await pickExecutable(kind);
      if (next) {
        setStatus(next);
        addLog(
          kind === "steam"
            ? "Steam location updated."
            : `Network target set to ${fileName(next.targetPath)}.`,
        );
      }
    });
  }

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <a
          href="#"
          className="brand"
          onClick={(e) => {
            e.preventDefault();
            setPage("library");
          }}
          aria-label="RBX Tools home"
        >
          <div className="brand-icon">
            <Gamepad2 size={24} />
          </div>
          <div>
            <strong>
              RBX<span>TOOLS</span>
            </strong>
            <small>YOUR LITTLE POWER-UPS</small>
          </div>
        </a>
        <div className="nav-label">PLAYER MENU</div>
        <nav aria-label="Main navigation">
          <button
            className={page === "library" ? "nav-item active" : "nav-item"}
            onClick={() => setPage("library")}
          >
            <Grid2X2 size={17} /> Tool library{" "}
            <span className="nav-count">01</span>
          </button>
          <button
            className={page === "steamy" ? "nav-item active" : "nav-item"}
            onClick={() => setPage("steamy")}
          >
            <Gamepad2 size={18} /> Steamy Friends{" "}
            {status.blocked && <i className="dot amber" />}
          </button>
        </nav>
        <div className="sidebar-bottom">
          <div className="session-box">
            <span className="eyebrow">
              <i className="dot" /> SESSION STATUS
            </span>
            <div>
              {status.blocked ? "Network is paused" : "All clear, player one."}
            </div>
            <p>
              {status.blocked
                ? "Restores automatically on exit."
                : "Your tools are ready when you are."}
            </p>
            <div className="session-divider" />
            <span className="admin-status">
              <ShieldCheck size={14} />
              {desktop
                ? status.elevated
                  ? "Administrator session"
                  : "Checking permissions"
                : "Browser preview"}
            </span>
          </div>
          <button className="nav-item" onClick={() => setModal("settings")}>
            <Settings2 size={17} /> Settings
          </button>
          <button className="nav-item" onClick={() => setModal("help")}>
            <CircleHelp size={17} /> Field guide <ArrowUpRight size={14} />
          </button>
          <div className="sidebar-version">
            <span>RBX TOOLS</span>
            <span>v0.2.3</span>
          </div>
        </div>
      </aside>

      <div className="workspace">
        <header className="topbar">
          <div className="breadcrumb">
            Workspace <ChevronRight size={13} />
            <span>
              {page === "library" ? "Tool library" : "Steamy Friends"}
            </span>
          </div>
          <div className="local-badge">
            <i className="dot" /> LOCAL & PERSONAL{" "}
            <span className="player-avatar">P1</span>
          </div>
        </header>
        <main>
          {!desktop && (
            <div className="preview-strip">
              <Radio size={14} /> Interface preview · Desktop actions are
              available in the Windows app.
            </div>
          )}
          {error && (
            <div role="alert" className="message error">
              <span>{error}</span>
              <button aria-label="Dismiss error" onClick={() => setError(null)}>
                <X size={16} />
              </button>
            </div>
          )}
          {notice && (
            <div role="status" className="message success">
              <Check size={16} />
              <span>{notice}</span>
              <button
                aria-label="Dismiss notification"
                onClick={() => setNotice(null)}
              >
                <X size={16} />
              </button>
            </div>
          )}

          {page === "library" ? (
            <>
              <div className="page-heading">
                <div>
                  <div className="eyebrow green">THE INVENTORY</div>
                  <h1>A little extra in your toolkit.</h1>
                  <p>
                    Small tools. Less friction. More time for the good stuff.
                  </p>
                </div>
                <span className="tag">
                  <span className="dot" /> 1 TOOL AVAILABLE
                </span>
              </div>
              <section className="hero">
                <div className="hero-copy">
                  <span className="pixel-label">READY, PLAYER ONE?</span>
                  <h2>
                    Your next session,
                    <br />
                    <span>with a few power-ups.</span>
                  </h2>
                  <p>
                    A pocket-sized collection of helpers for the games
                    <br className="wide-break" /> you play. Pick a tool and get
                    back to it.
                  </p>
                  <button
                    className="hero-link"
                    onClick={() => setPage("steamy")}
                  >
                    LET’S PLAY <ArrowRight size={17} />
                  </button>
                </div>
                <Landscape />
                <div className="hero-coordinates">
                  LVL. 01 <span>•</span> HOME BASE
                </div>
              </section>
              <div className="section-heading">
                <h3>
                  <Grid2X2 size={16} /> Your tools <span>01</span>
                </h3>
                <span>SELECT YOUR POWER-UP</span>
              </div>
              <div className="tools-grid">
                <button className="tool-card" onClick={() => setPage("steamy")}>
                  <div className="tool-art">
                    <div className="art-grid" />
                    <span className="cartridge-label">UTILITY / 001</span>
                    <div className="friend-orbit">
                      <PixelFriend />
                    </div>
                    <span className="art-status">
                      <i className={`dot ${status.blocked ? "amber" : ""}`} />
                      {status.blocked ? "NETWORK PAUSED" : "READY TO USE"}
                    </span>
                    <span className="sparkle s1">+</span>
                    <span className="sparkle s2">+</span>
                  </div>
                  <div className="tool-body">
                    <div className="tool-title">
                      <h3>Steamy Friends</h3>
                      <span className="tool-arrow">
                        <ArrowUpRight size={18} />
                      </span>
                    </div>
                    <p>
                      A fresh start for Steam. A little breathing
                      <br className="wide-break" /> room for your connection.
                    </p>
                    <div className="tool-tags">
                      <span>
                        <RotateCw size={12} /> RESTART STEAM
                      </span>
                      <span>
                        <Wifi size={12} /> NETWORK CONTROL
                      </span>
                    </div>
                  </div>
                </button>
                <div className="empty-slot">
                  <div className="slot-icon">+</div>
                  <h3>Room for more.</h3>
                  <p>
                    Your next handy little tool
                    <br />
                    belongs right here.
                  </p>
                  <span>EMPTY SLOT</span>
                </div>
              </div>
              <div className="bottom-note">
                <ShieldCheck size={19} />
                <div>
                  <strong>Leave things how you found them.</strong>
                  <p>
                    Temporary changes are restored when you close RBX Tools.
                  </p>
                </div>
                <span className="pixel-label">GOOD GAME.</span>
              </div>
            </>
          ) : (
            <>
              <button className="back-link" onClick={() => setPage("library")}>
                <ArrowLeft size={15} /> Back to your tools
              </button>
              <div className="page-heading tool-page-heading">
                <div>
                  <div className="eyebrow green">UTILITY / 001</div>
                  <h1>
                    Steamy Friends
                    <span className="inline-friend">
                      <PixelFriend small />
                    </span>
                  </h1>
                  <p>A fresh start. A quiet connection. You're in control.</p>
                </div>
                <span className="tag">
                  <i className={`dot ${status.blocked ? "amber" : ""}`} />
                  {status.blocked ? "NETWORK PAUSED" : "SESSION READY"}
                </span>
              </div>
              <div className="control-grid">
                <section className="control-card">
                  <div className="control-icon">
                    <RotateCw size={24} />
                  </div>
                  <span className="eyebrow">QUICK RESET</span>
                  <h2>Restart Steam</h2>
                  <p>
                    Close Steam, then bring it right back.
                    <br />
                    One button. Fresh session.
                  </p>
                  <div className="control-status">
                    <i
                      className={`dot ${status.steamRunning ? "" : "muted"}`}
                    />
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
                    disabled={
                      !!busy || loading || !desktop || !status.steamPath
                    }
                    onClick={() => restart()}
                  >
                    {busy === "restart" ? (
                      <LoaderCircle className="spin" size={16} />
                    ) : (
                      <RotateCw size={16} />
                    )}{" "}
                    {busy === "restart" ? "Restarting Steam…" : "Restart Steam"}
                  </button>
                  <button
                    className="text-button"
                    disabled={!!busy}
                    onClick={() => choose("steam")}
                  >
                    <FolderOpen size={13} />
                    {status.steamPath
                      ? "Change Steam location"
                      : "Locate steam.exe"}
                  </button>
                </section>
                <section
                  className={`control-card network-card ${status.blocked ? "is-blocked" : ""}`}
                >
                  <div className="control-icon">
                    {status.blocked ? (
                      <WifiOff size={24} />
                    ) : (
                      <Wifi size={24} />
                    )}
                  </div>
                  <span className="eyebrow">CONNECTION BREAK</span>
                  <h2>{status.blocked ? "Network paused" : "Stop network"}</h2>
                  <p>
                    Pause incoming and outgoing traffic
                    <br />
                    for steam.exe only.
                  </p>
                  <div className="control-status">
                    <i
                      className={`dot ${status.blocked ? "amber" : status.targetPath ? "" : "muted"}`}
                    />
                    {status.blocked
                      ? "Incoming & outgoing blocked"
                      : status.targetPath
                        ? "No RBX block active"
                        : "Locate your Steam installation first"}
                  </div>
                  <button
                    className={
                      status.blocked
                        ? "primary-button amber-button"
                        : "secondary-button"
                    }
                    disabled={
                      !!busy || loading || !desktop || !status.targetPath
                    }
                    onClick={toggleNetwork}
                  >
                    {busy === "network" ? (
                      <LoaderCircle className="spin" size={16} />
                    ) : status.blocked ? (
                      <Wifi size={16} />
                    ) : (
                      <WifiOff size={16} />
                    )}{" "}
                    {busy === "network"
                      ? status.blocked
                        ? "Restoring & reconnecting…"
                        : "Pausing connection…"
                      : status.blocked
                        ? "Restore & reconnect"
                        : "Stop network"}
                  </button>
                  <div className="tiny-note">
                    <ShieldCheck size={13} /> Automatically restores when you
                    exit
                  </div>
                </section>
              </div>
              <section className="target-panel">
                <div className="target-heading">
                  <div>
                    <h3>Network target</h3>
                    <p>Only steam.exe · matches your NetLimiter rule.</p>
                  </div>
                  <button
                    className="small-button"
                    disabled={!!busy || status.blocked}
                    onClick={() => choose("target")}
                  >
                    <FolderOpen size={14} />
                    {status.targetPath
                      ? "Change Steam location"
                      : "Locate steam.exe"}
                  </button>
                </div>
                <div className="target-file">
                  <div className="exe-icon">EXE</div>
                  <div>
                    <strong>{fileName(status.targetPath)}</strong>
                    <p title={status.targetPath ?? undefined}>
                      {status.targetPath ??
                        "Steam was not detected. Locate steam.exe to enable network controls."}
                    </p>
                  </div>
                  <span className="direction-badge">
                    <ArrowDownLeft size={12} /> IN <ArrowUpRight size={12} />{" "}
                    OUT
                  </span>
                </div>
              </section>
              <section className="activity-panel">
                <div className="section-heading">
                  <h3>
                    <Radio size={15} /> Session activity
                  </h3>
                  <span>THIS SESSION ONLY</span>
                </div>
                {logs.length ? (
                  logs.map((log, i) => (
                    <div className="log-row" key={`${log.time}-${i}`}>
                      <span className="log-bullet" />
                      <span>{log.message}</span>
                      <time>{log.time}</time>
                    </div>
                  ))
                ) : (
                  <div className="activity-empty">
                    Nothing to report. Your next action will appear here.
                  </div>
                )}
              </section>
              <div className="bottom-note compact">
                <ShieldCheck size={18} />
                <p>
                  Only steam.exe is affected. Games and Steam helpers are not
                  targeted.
                </p>
              </div>
            </>
          )}
          <footer>
            <span>
              <span className="footer-pixel">✦</span> MADE FOR YOUR BETWEEN-GAME
              MOMENTS
            </span>
            <span>LOCAL TOOLS. NO ACCOUNTS.</span>
          </footer>
        </main>
      </div>

      {modal && (
        <div
          className="modal-backdrop"
          onClick={() => {
            if (modal !== "force") setModal(null);
          }}
        >
          <section
            ref={dialogRef}
            className="modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="modal-title"
            onClick={(e) => e.stopPropagation()}
            onKeyDown={(e) => {
              if (e.key === "Escape") setModal(null);
            }}
          >
            <button
              className="modal-close"
              aria-label="Close dialog"
              onClick={() => setModal(null)}
              autoFocus
            >
              <X size={19} />
            </button>
            {modal === "settings" ? (
              <>
                <Settings2 className="green" size={25} />
                <h2 id="modal-title">Your setup</h2>
                <p className="modal-intro">
                  A small toolkit, with just the essentials.
                </p>
                <div className="setting-row">
                  <div>
                    <strong>Appearance</strong>
                    <p>GBA-inspired, red after dark.</p>
                  </div>
                  <span className="tag">DARK</span>
                </div>
                <div className="setting-row">
                  <div>
                    <strong>Restore on exit</strong>
                    <p>Temporary network filters end with the app.</p>
                  </div>
                  <span className="locked-setting">
                    <LockKeyhole size={13} /> Always on
                  </span>
                </div>
                <div className="setting-row">
                  <div>
                    <strong>Steam location</strong>
                    <p className="path-label">
                      {status.steamPath ?? "Not detected yet"}
                    </p>
                  </div>
                  <button
                    className="small-button"
                    disabled={!!busy}
                    onClick={() => choose("steam")}
                  >
                    Browse
                  </button>
                </div>
                <div className="setting-row">
                  <div>
                    <strong>
                      {localUpdate
                        ? `Update v${localUpdate.version} ready`
                        : "Shared Builds update"}
                    </strong>
                    <p>
                      {localUpdate
                        ? localUpdate.notes || "Ready from your shared OneDrive folder."
                        : "Check the shared OneDrive Builds folder."}
                    </p>
                  </div>
                  <button
                    className="small-button"
                    disabled={!!busy || !desktop}
                    onClick={localUpdate ? installSharedUpdate : checkForSharedUpdate}
                  >
                    {busy === "install-update"
                      ? "Starting…"
                      : busy === "check-update"
                        ? "Checking…"
                        : localUpdate
                          ? "Install"
                          : "Check now"}
                  </button>
                </div>
                <div className="setting-row">
                  <div>
                    <strong>Menu sounds</strong>
                    <p>Quiet by default.</p>
                  </div>
                  <Volume2 size={17} />
                </div>
                <p className="modal-footnote">
                  RBX Tools v0.2.3 · Windows desktop edition
                </p>
              </>
            ) : modal === "help" ? (
              <>
                <Gamepad2 className="green" size={28} />
                <h2 id="modal-title">Field guide</h2>
                <p className="modal-intro">
                  A few things before your next session.
                </p>
                <ol className="guide-list">
                  <li>
                    <strong>Restart Steam</strong>
                    <p>
                      Steam gets time to exit cleanly. If it stays open, you can
                      choose whether to force-close it. Save your game and
                      finish cloud sync first.
                    </p>
                  </li>
                  <li>
                    <strong>Steam network control</strong>
                    <p>
                      The block targets your detected steam.exe, matching your
                      NetLimiter rule. Games and Steam helper processes are not
                      included.
                    </p>
                  </li>
                  <li>
                    <strong>Restore whenever you want</strong>
                    <p>
                      Click Restore & reconnect or close RBX Tools. Windows also
                      removes these session filters if the app crashes. RBX
                      Tools checks Steam after long pauses and requests fresh
                      account data to trigger an immediate reconnect if needed.
                    </p>
                  </li>
                </ol>
              </>
            ) : (
              <>
                <RotateCw className="green" size={26} />
                <h2 id="modal-title">Steam is still running</h2>
                <p className="modal-intro">
                  Steam did not finish closing. Force-close the selected Steam
                  process and restart it? This can interrupt downloads and cloud
                  sync.
                </p>
                <div className="modal-actions">
                  <button
                    className="secondary-button"
                    onClick={() => setModal(null)}
                  >
                    Cancel
                  </button>
                  <button
                    className="primary-button"
                    onClick={() => restart(true)}
                  >
                    Force restart
                  </button>
                </div>
              </>
            )}
          </section>
        </div>
      )}
    </div>
  );
}
