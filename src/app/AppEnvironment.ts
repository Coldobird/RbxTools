import type { Desktop } from "../platform/Desktop";
import type { PageVisibility } from "../platform/PageVisibility";
import type { Timings } from "../platform/Timings";

/** Turns the UI test view on and off; `TestViewMode` is the real one. */
export interface TestViewSwitch {
  readonly isActive: boolean;
  set(active: boolean): void;
}

/** Everything outside the app logic that the controller depends on, as narrow as it is used. */
export interface AppEnvironment {
  readonly desktop: Desktop;
  readonly testView: TestViewSwitch;
  readonly timings: Pick<Timings, "markOnce">;
  readonly visibility: Pick<PageVisibility, "isHidden" | "onVisible">;
}
