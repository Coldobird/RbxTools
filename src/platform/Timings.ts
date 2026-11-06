const SLOW_COMMAND_MS = 1_000;

/** One timing, on the process-start timeline (`at` in epoch milliseconds). */
export interface TimingEntry {
  readonly name: string;
  readonly at: number;
  readonly duration?: number;
  readonly detail?: string;
}

type MeasuredEntry = PerformanceEntry & { value: number; hadRecentInput: boolean; element?: Element };

/**
 * Logs app timings to the console and, once connected, to the desktop
 * `timings.log`, so open, paint and command timings share one origin.
 */
export class Timings {
  private readonly logged = new Set<string>();
  private started = false;
  /** Entries recorded before the desktop log was connected. */
  private pending: TimingEntry[] | null = [];
  private sink: ((entry: TimingEntry) => void) | null = null;

  /** Sends every entry, including the earlier ones, to the desktop log. */
  sendTo(sink: (entry: TimingEntry) => void): void {
    this.sink = sink;
    for (const entry of this.pending ?? []) sink(entry);
    this.pending = null;
  }

  /** Records an event at `startTime` (ms since navigation start). */
  log(name: string, startTime = performance.now(), duration?: number, detail?: string): void {
    if (!this.started) return;
    const at = performance.timeOrigin + startTime;
    const length = duration === undefined ? "" : ` ${duration.toFixed(1)}ms`;
    console.info(`[timing] ${name} @${startTime.toFixed(1)}ms${length}${detail ? ` ${detail}` : ""}`);
    const entry = { name, at, duration, detail };
    if (this.sink) this.sink(entry);
    else this.pending?.push(entry);
  }

  /** Records a milestone only the first time it is reached. */
  markOnce(name: string, detail?: string): void {
    if (this.logged.has(name)) return;
    this.logged.add(name);
    this.log(name, performance.now(), undefined, detail);
  }

  /** Times a backend command. Polled commands are logged only on first call or when slow. */
  async timeCommand<T>(command: string, run: () => Promise<T>, polled = false): Promise<T> {
    const start = performance.now();
    let outcome = "ok";
    try {
      return await run();
    } catch (error) {
      outcome = `error: ${String(error)}`;
      throw error;
    } finally {
      const duration = performance.now() - start;
      const key = `command.${command}`;
      const first = !this.logged.has(key);
      this.logged.add(key);
      if (!polled || first || duration >= SLOW_COMMAND_MS) this.log(key, start, duration, outcome);
    }
  }

  /** Starts logging navigation, paint, LCP, CLS, long tasks, and slow interactions. */
  start(): void {
    if (this.started) return;
    this.started = true;
    this.log("nav.timeOrigin", 0);
    this.logNavigationWhenLoaded();
    this.observe("paint", entries => entries.forEach(entry => this.log(`paint.${entry.name}`, entry.startTime)));
    this.observe("longtask", entries => entries.forEach(entry => this.log("longtask", entry.startTime, entry.duration)));
    this.observeVitals();
  }

  private logNavigationWhenLoaded(): void {
    const logNavigation = () => {
      const nav = performance.getEntriesByType("navigation")[0] as PerformanceNavigationTiming | undefined;
      if (!nav) return;
      this.log("nav.responseEnd", nav.responseEnd);
      this.log("nav.domInteractive", nav.domInteractive);
      this.log("nav.domContentLoaded", nav.domContentLoadedEventEnd);
      this.log("nav.load", nav.loadEventEnd);
    };
    const navigation = performance.getEntriesByType("navigation")[0] as PerformanceNavigationTiming | undefined;
    if (navigation?.loadEventEnd) logNavigation();
    else window.addEventListener("load", () => setTimeout(logNavigation), { once: true });
  }

  private observeVitals(): void {
    let lcp: MeasuredEntry | undefined;
    let cls = 0;
    let inp = 0;
    this.observe("largest-contentful-paint", entries => { lcp = (entries.at(-1) as MeasuredEntry | undefined) ?? lcp; });
    this.observe("layout-shift", entries => {
      for (const entry of entries as MeasuredEntry[]) if (!entry.hadRecentInput) cls += entry.value;
    });
    this.observe("event", entries => {
      for (const entry of entries as PerformanceEventTiming[]) {
        if (entry.duration < 100) continue;
        inp = Math.max(inp, entry.duration);
        this.log(`interaction.${entry.name}`, entry.startTime, entry.duration);
      }
    });

    // LCP and CLS are final once the user interacts or the page is hidden.
    let finalized = false;
    const finalize = () => {
      if (finalized) return;
      finalized = true;
      if (lcp) this.log("lcp", lcp.startTime, undefined, lcp.element?.tagName.toLowerCase());
      this.log("cls", performance.now(), undefined, cls.toFixed(4));
    };
    for (const type of ["keydown", "pointerdown"]) addEventListener(type, finalize, { once: true, capture: true });
    document.addEventListener("visibilitychange", () => {
      if (!document.hidden) return;
      finalize();
      if (inp) this.log("inp.max", performance.now(), inp);
    });
    // An untouched desktop window may never be hidden, so settle startup values after 10 seconds.
    setTimeout(finalize, 10_000);
  }

  private observe(type: string, onEntries: (entries: PerformanceEntryList) => void): void {
    if (!PerformanceObserver.supportedEntryTypes?.includes(type)) return;
    new PerformanceObserver(list => onEntries(list.getEntries())).observe({ type, buffered: true });
  }
}

