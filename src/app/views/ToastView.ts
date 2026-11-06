import type { AppState, Notice } from "../AppState";
import type { AppAbout } from "./AppAbout";
import { StateView } from "./StateView";
import { UpdateView } from "./UpdateView";

/** What the error and notice toasts show. */
export class ToastView extends StateView {
  readonly update: UpdateView;

  constructor(state: AppState, private readonly about: AppAbout) {
    super(state);
    this.update = new UpdateView(state);
  }

  get error(): string | null {
    return this.state.error;
  }

  get notice(): Notice | null {
    return this.state.notice;
  }

  /** Desktop and test-view errors come from the system; browser-preview ones explain the preview. */
  get isSystemError(): boolean {
    return this.about.isDesktop || this.state.testView;
  }

  get showsUpdateButton(): boolean {
    return !!this.notice?.offersUpdate && this.update.version !== null;
  }
}
