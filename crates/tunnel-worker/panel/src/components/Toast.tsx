interface TokenToastProps {
  token: string;
  onDismiss: () => void;
}

export function TokenToast({ token, onDismiss }: TokenToastProps) {
  async function copy() {
    try {
      await navigator.clipboard.writeText(token);
    } catch {
      // Clipboard API needs a secure context; the token stays visible to copy by hand.
    }
  }
  return (
    <div class="toast">
      <span>copy this token now — it is shown only once:</span>
      <code class="accent">{token}</code>
      <button class="btn" onClick={copy}>
        copy
      </button>
      <button class="btn" onClick={onDismiss}>
        dismiss
      </button>
    </div>
  );
}
