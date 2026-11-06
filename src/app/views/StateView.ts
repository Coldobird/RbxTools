import type { ActionName } from "../../domain/RunningActions";
import type { AppState } from "../AppState";

/** A read-only view of one app state; each subclass presents one part of the window. */
export abstract class StateView {
  constructor(protected readonly state: AppState) {}

  /** Any action is running. */
  get isBusy(): boolean {
    return !this.state.running.isIdle;
  }

  protected isRunning(action: ActionName): boolean {
    return this.state.running.has(action);
  }
}
