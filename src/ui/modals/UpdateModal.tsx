import type { AppController } from "../../app/AppController";
import type { UpdateView } from "../../app/views/UpdateView";
import { ModalHeading } from "../components/ModalFrame";
import { IconRestart } from "../components/PixelIcons";

/** Offered on startup when GitHub has a newer release. */
export function UpdateModal({ view, actions }: { view: UpdateView; actions: Pick<AppController, "closeModal" | "installUpdate"> }) {
  return (
    <>
      <ModalHeading icon={<IconRestart className="accent" size={26} />} title="Update available" />
      <p className="modal-intro">
        RBX Tools v{view.version} is ready. Update from GitHub and restart the app to finish installing.
      </p>
      {view.error && <p role="alert" className="modal-intro">{view.error}</p>}
      <div className="modal-actions">
        <button className="secondary-button" disabled={view.isInstalling} onClick={() => actions.closeModal()}>
          <span className="ui-label">Cancel</span>
        </button>
        <button className="primary-button" disabled={!view.canInstall} onClick={() => actions.installUpdate()}>
          <span className="ui-label">{view.installButtonLabel}</span>
        </button>
      </div>
    </>
  );
}
