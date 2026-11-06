import type { AppController } from "../../app/AppController";
import type { ToastView } from "../../app/views/ToastView";
import { IconAlert, IconCheck, IconClose, IconPreview } from "./PixelIcons";

type ToastActions = Pick<AppController, "dismissError" | "dismissNotice" | "installUpdate">;

/** Error and update notices, stacked in the corner. */
export function ToastStack({ view, actions }: { view: ToastView; actions: ToastActions }) {
  const { error, notice } = view;
  if (!error && !notice) return null;
  return (
    <div className="toast-stack">
      {error && (
        <div role="alert" className="toast error">
          {view.isSystemError ? <IconAlert size={17} /> : <IconPreview size={17} />}
          <span className="ui-label">{error}</span>
          <button aria-label="Dismiss error" onClick={() => actions.dismissError()}><IconClose size={16} /></button>
        </div>
      )}
      {notice && (
        <div role="status" className={notice.offersUpdate ? "toast success update-notice" : "toast success"}>
          <IconCheck size={17} />
          <span className="ui-label">{notice.message}</span>
          {view.showsUpdateButton && (
            <button className="small-button toast-update" disabled={!view.update.canInstall} onClick={() => actions.installUpdate()}>
              <span className="ui-label">Update</span>
            </button>
          )}
          <button aria-label="Dismiss notification" onClick={() => actions.dismissNotice()}><IconClose size={16} /></button>
        </div>
      )}
    </div>
  );
}
