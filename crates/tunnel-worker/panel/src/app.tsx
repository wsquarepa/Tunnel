import { useState } from "preact/hooks";
import { send } from "./api";
import { Login } from "./components/Login";
import { Clients } from "./components/Clients";
import { Routes } from "./components/Routes";

export function App() {
  const [authed, setAuthed] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [changeTick, setChangeTick] = useState(0);

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
          <Clients
            selectedId={selected}
            onSelect={setSelected}
            onChanged={() => setChangeTick((n) => n + 1)}
          />
          <Routes changeTick={changeTick} />
        </div>
      ) : (
        <Login onAuthed={() => setAuthed(true)} />
      )}
    </main>
  );
}
