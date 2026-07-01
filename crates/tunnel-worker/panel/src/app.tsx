import { useState } from "preact/hooks";
import { send } from "./api";
import { Login } from "./components/Login";

export function App() {
  const [authed, setAuthed] = useState(false);

  async function logout() {
    try {
      await send("/admin/logout", "POST");
    } finally {
      setAuthed(false);
    }
  }

  return (
    <main class="wrap">
      <div class="bar">
        <span class="brand">
          ◈ <b>tunnel</b> admin
        </span>
        {authed && (
          <button class="btn" onClick={logout}>
            logout
          </button>
        )}
      </div>
      {authed ? (
        <div class="cols">
          <p class="muted">clients + routes added in the next tasks.</p>
        </div>
      ) : (
        <Login onAuthed={() => setAuthed(true)} />
      )}
    </main>
  );
}
