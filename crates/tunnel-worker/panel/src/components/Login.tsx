import { useState } from "preact/hooks";
import { send } from "../api";
import { Field } from "./Field";

interface LoginProps {
  onAuthed: () => void;
}

export function Login({ onAuthed }: LoginProps) {
  const [secret, setSecret] = useState("");
  const [error, setError] = useState("");

  async function submit(e: Event) {
    e.preventDefault();
    setError("");
    try {
      await send("/admin/login", "POST", { secret });
      onAuthed();
    } catch {
      setError("login failed — check the admin secret");
    }
  }

  return (
    <form class="row-form" onSubmit={submit} style="margin-top:2rem">
      <Field label="Admin secret" hint="the ADMIN_SECRET set on the Worker">
        <input
          class="inp"
          type="password"
          value={secret}
          onInput={(e) => setSecret((e.target as HTMLInputElement).value)}
          placeholder="••••••••"
        />
      </Field>
      <button class="btn btn-accent" type="submit">
        log in
      </button>
      {error && <span class="err">{error}</span>}
    </form>
  );
}
