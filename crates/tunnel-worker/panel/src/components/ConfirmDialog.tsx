import { useEffect, useRef } from "preact/hooks";

interface ConfirmDialogProps {
  message: string;
  confirmLabel: string;
  onConfirm: () => void;
  // Fires on any close (cancel button, Esc, or after a confirm). The parent uses
  // it to clear the "pending confirmation" state; onConfirm has already run the
  // action by then when the user confirmed.
  onClose: () => void;
}

export function ConfirmDialog({ message, confirmLabel, onConfirm, onClose }: ConfirmDialogProps) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    ref.current?.showModal();
  }, []);

  return (
    <dialog ref={ref} class="dialog" onClose={onClose}>
      <p>{message}</p>
      <div class="dialog-actions">
        <button class="btn" onClick={() => ref.current?.close()}>
          cancel
        </button>
        <button
          class="btn btn-danger"
          onClick={() => {
            onConfirm();
            ref.current?.close();
          }}
        >
          {confirmLabel}
        </button>
      </div>
    </dialog>
  );
}
