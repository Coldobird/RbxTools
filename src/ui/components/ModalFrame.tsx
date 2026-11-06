import { useEffect, useRef, useState, type ReactNode } from "react";
import { IconClose } from "./PixelIcons";

const focusable = 'button:not(:disabled), a[href], input:not(:disabled), [tabindex="0"]';

/**
 * A modal dialog: traps Tab inside, closes on Escape or a backdrop click when
 * allowed, and returns focus to where it was when the dialog closes.
 */
export function ModalFrame({ children, dismissible, closeDisabled, onDismiss, onClose, ignoreFocusFrom }: {
  children: ReactNode;
  /** Whether Escape and backdrop clicks close the dialog. */
  dismissible: boolean;
  closeDisabled: boolean;
  onDismiss: () => void;
  onClose: () => void;
  /** Keyboard events from inside this selector are left alone (the UI test panel). */
  ignoreFocusFrom?: string;
}) {
  const dialogRef = useRef<HTMLElement>(null);
  const [returnFocus] = useState(() => document.activeElement as HTMLElement | null);

  useEffect(() => {
    const trapFocus = (event: KeyboardEvent) => {
      const dialog = dialogRef.current;
      if (ignoreFocusFrom && event.target instanceof Element && event.target.closest(ignoreFocusFrom)) return;
      if (event.key !== "Tab" || !dialog) return;
      const controls = [...dialog.querySelectorAll<HTMLElement>(focusable)];
      const first = controls[0];
      const last = controls[controls.length - 1];
      const outside = !dialog.contains(document.activeElement);
      if (event.shiftKey && (document.activeElement === first || outside)) {
        event.preventDefault();
        last?.focus();
      } else if (!event.shiftKey && (document.activeElement === last || outside)) {
        event.preventDefault();
        first?.focus();
      }
    };
    document.addEventListener("keydown", trapFocus);
    return () => {
      document.removeEventListener("keydown", trapFocus);
      returnFocus?.focus();
    };
  }, [ignoreFocusFrom, returnFocus]);

  return (
    <div className="modal-backdrop" onClick={() => { if (dismissible) onDismiss(); }}>
      <section
        ref={dialogRef}
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="modal-title"
        onClick={event => event.stopPropagation()}
        onKeyDown={event => { if (event.key === "Escape" && dismissible) onDismiss(); }}
      >
        <button className="modal-close" aria-label="Close dialog" disabled={closeDisabled} onClick={onClose} autoFocus>
          <IconClose size={19} />
        </button>
        {children}
      </section>
    </div>
  );
}

/** A dialog's icon and title. */
export function ModalHeading({ icon, title }: { icon: ReactNode; title: string }) {
  return (
    <div className="modal-heading">
      {icon}
      <h2 id="modal-title">{title}</h2>
    </div>
  );
}
