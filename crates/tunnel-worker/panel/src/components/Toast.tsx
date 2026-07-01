import { useEffect, useRef } from "preact/hooks";

interface TokenDialogProps {
  token: string;
  onDismiss: () => void;
}

export function TokenDialog({ token, onDismiss }: TokenDialogProps) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    ref.current?.showModal();
  }, []);

  async function copy() {
    try {
      await navigator.clipboard.writeText(token);
    } catch {
      // Clipboard API needs a secure context; the token stays visible to copy by hand.
    }
  }

  return (
    <dialog ref={ref} class="dialog" onClose={onDismiss}>
      <p>copy this token now, it will not be shown again:</p>
      <div class="token-row">
        <code class="token">{token}</code>
        <button class="btn" onClick={copy}>
          copy
        </button>
      </div>
      <div class="dialog-actions">
        <button class="btn" onClick={() => ref.current?.close()}>
          dismiss
        </button>
      </div>
    </dialog>
  );
}
