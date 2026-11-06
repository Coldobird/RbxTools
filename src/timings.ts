import { invoke, isTauri } from "@tauri-apps/api/core";

/** Logs app timings to the desktop timings.log (and the console) on the process-start timeline. */

const desktop = isTauri();
const logged = new Set<string>();
let started = false;

export function logTiming(name: string, startTime = performance.now(), duration?: number, detail?: string) {
  if (!started) return;
  const at = performance.timeOrigin + startTime;
  console.info(`[timing] ${name} @${startTime.toFixed(1)}ms${duration === undefined ? "" : ` ${duration.toFixed(1)}ms`}${detail ? ` ${detail}` : ""}`);
  if (desktop) void invoke("log_timing", { name, at, duration, detail }).catch(() => undefined);
}

/** Logs a milestone only the first time it is reached. */
export function markOnce(name: string, detail?: string) {
  if (logged.has(name)) return;
  logged.add(name);
  logTiming(name, performance.now(), undefined, detail);
}

const SLOW_COMMAND_MS = 1_000;

/** Times a backend command; repeated polling commands are logged only on first call or when slow. */
export async function timedCommand<T>(command: string, run: () => Promise<T>, polled = false): Promise<T> {
  const start = performance.now();
  let outcome = "ok";
  try {
    return await run();
  } catch (error) {
    outcome = `error: ${String(error)}`;
    throw error;
  } finally {
    const duration = performance.now() - start;
    const first = !logged.has(`command.${command}`);
    logged.add(`command.${command}`);
    if (!polled || first || duration >= SLOW_COMMAND_MS)
      logTiming(`command.${command}`, start, duration, outcome);
  }
}

function observe(type: string, onEntries: (entries: PerformanceEntryList) => void) {
  if (!PerformanceObserver.supportedEntryTypes?.includes(type)) return;
  new PerformanceObserver(list => onEntries(list.getEntries())).observe({ type, buffered: true });
}

/** Starts logging navigation, paint, LCP, CLS, long tasks, and slow interactions. */
export function startTimingLog() {
  if (started) return;
  started = true;

  const navigation = performance.getEntriesByType("navigation")[0] as PerformanceNavigationTiming | undefined;
  const logNavigation = () => {
    const nav = performance.getEntriesByType("navigation")[0] as PerformanceNavigationTiming | undefined;
    if (!nav) return;
    logTiming("nav.responseEnd", nav.responseEnd);
    logTiming("nav.domInteractive", nav.domInteractive);
    logTiming("nav.domContentLoaded", nav.domContentLoadedEventEnd);
    logTiming("nav.load", nav.loadEventEnd);
  };
  logTiming("nav.timeOrigin", 0);
  if (navigation?.loadEventEnd) logNavigation();
  else window.addEventListener("load", () => setTimeout(logNavigation), { once: true });

  observe("paint", entries => entries.forEach(entry => logTiming(`paint.${entry.name}`, entry.startTime)));

  let lcp: PerformanceEntry | undefined;
  observe("largest-contentful-paint", entries => { lcp = entries.at(-1) ?? lcp; });

  let cls = 0;
  observe("layout-shift", entries => {
    for (const entry of entries as (PerformanceEntry & { value: number; hadRecentInput: boolean })[])
      if (!entry.hadRecentInput) cls += entry.value;
  });

  observe("longtask", entries => entries.forEach(entry => logTiming("longtask", entry.startTime, entry.duration)));

  let inp = 0;
  observe("event", entries => {
    for (const entry of entries as PerformanceEventTiming[]) {
      if (entry.duration < 100) continue;
      inp = Math.max(inp, entry.duration);
      logTiming(`interaction.${entry.name}`, entry.startTime, entry.duration);
    }
  });

  // LCP and CLS are final once the user interacts or the page is hidden.
  let finalized = false;
  const finalize = () => {
    if (finalized) return;
    finalized = true;
    if (lcp) logTiming("lcp", lcp.startTime, undefined, (lcp as PerformanceEntry & { element?: Element }).element?.tagName.toLowerCase());
    logTiming("cls", performance.now(), undefined, cls.toFixed(4));
  };
  for (const type of ["keydown", "pointerdown"]) addEventListener(type, finalize, { once: true, capture: true });
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) {
      finalize();
      if (inp) logTiming("inp.max", performance.now(), inp);
    }
  });
  // An untouched desktop window may never be hidden, so settle the startup values after 10 seconds.
  setTimeout(finalize, 10_000);
}
