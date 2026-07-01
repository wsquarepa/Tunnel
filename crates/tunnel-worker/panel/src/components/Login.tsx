import { useState } from "preact/hooks";
import { send } from "../api";
import { notify } from "../toast";

interface LoginProps {
  onAuthed: () => void;
}

export function Login({ onAuthed }: LoginProps) {
  const [secret, setSecret] = useState("");

  async function submit(e: Event) {
    e.preventDefault();
    try {
      await send("/admin/login", "POST", { secret });
      onAuthed();
    } catch {
      notify("login failed, check the admin secret");
    }
  }

  return (
    <form class="form" onSubmit={submit} style="margin-top:2rem;max-width:24rem">
      <span class="field-label">Admin secret</span>
      <div class="input-row">
        <input
          class="inp"
          type="password"
          value={secret}
          onInput={(e) => setSecret((e.target as HTMLInputElement).value)}
        />
        <button class="btn btn-accent" type="submit">
          log in
        </button>
      </div>
      <span class="field-hint">the ADMIN_SECRET set on the Worker</span>
    </form>
  );
}
