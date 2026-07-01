import { useEffect, useState } from "preact/hooks";

// A tiny module-level toast bus so any component can raise a notification
// without prop-drilling. Toasts render top-center via <ToastHost/>.
export type ToastKind = "error" | "info";

interface Toast {
  id: number;
  message: string;
  kind: ToastKind;
  leaving: boolean;
}

const AUTO_DISMISS_MS = 5000;
const LEAVE_ANIM_MS = 220;

let toasts: Toast[] = [];
let seq = 0;
const listeners = new Set<() => void>();

function emit(): void {
  for (const listener of listeners) listener();
}

function dismiss(id: number): void {
  toasts = toasts.map((t) => (t.id === id ? { ...t, leaving: true } : t));
  emit();
  setTimeout(() => {
    toasts = toasts.filter((t) => t.id !== id);
    emit();
  }, LEAVE_ANIM_MS);
}

export function notify(message: string, kind: ToastKind = "error"): void {
  const id = ++seq;
  toasts = [...toasts, { id, message, kind, leaving: false }];
  emit();
  setTimeout(() => dismiss(id), AUTO_DISMISS_MS);
}

export function ToastHost() {
  const [, force] = useState(0);
  useEffect(() => {
    const listener = () => force((n) => n + 1);
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }, []);

  return (
    <div class="toast-host">
      {toasts.map((t) => (
        <div
          key={t.id}
          class={`toast-msg toast-${t.kind}${t.leaving ? " leaving" : ""}`}
          onClick={() => dismiss(t.id)}
        >
          {t.message}
        </div>
      ))}
    </div>
  );
}
