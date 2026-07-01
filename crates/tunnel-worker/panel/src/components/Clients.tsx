import { useEffect, useState } from "preact/hooks";
import { getJson, send } from "../api";
import type { Client, CreatedClient } from "../types";
import { notify } from "../toast";
import { TokenDialog } from "./Toast";
import { ConfirmDialog } from "./ConfirmDialog";

interface ClientsProps {
  selectedId: string | null;
  onSelect: (id: string | null) => void;
  onChanged: () => void;
}

const REFRESH_MS = 5000;

export function Clients({ selectedId, onSelect, onChanged }: ClientsProps) {
  const [clients, setClients] = useState<Client[]>([]);
  const [online, setOnline] = useState<Record<string, boolean>>({});
  const [name, setName] = useState("");
  const [newToken, setNewToken] = useState<string | null>(null);
  const [pendingDelete, setPendingDelete] = useState<Client | null>(null);

  // The client list rarely changes, so it is loaded on mount and after
  // mutations. Live connection state comes from a single aggregate endpoint that
  // fans out to the Durable Objects server-side, polled on the interval below.
  async function load() {
    setClients(await getJson<Client[]>("/admin/clients"));
  }
  async function refreshStatus() {
    try {
      setOnline(await getJson<Record<string, boolean>>("/admin/clients/status"));
    } catch {
      // Keep the last-known status on a transient fetch failure.
    }
  }
  async function reload() {
    await load();
    await refreshStatus();
  }

  useEffect(() => {
    let live = true;
    async function tick() {
      try {
        const conn = await getJson<Record<string, boolean>>("/admin/clients/status");
        if (live) setOnline(conn);
      } catch {
        // Keep the last-known status on a transient fetch failure.
      }
    }
    void load();
    void tick();
    const id = setInterval(tick, REFRESH_MS);
    return () => {
      live = false;
      clearInterval(id);
    };
  }, []);

  async function create(e: Event) {
    e.preventDefault();
    if (!name.trim()) return;
    try {
      const created = await send<CreatedClient>("/admin/clients", "POST", { name: name.trim() });
      setNewToken(created.token);
      setName("");
      await reload();
      onChanged();
    } catch (e) {
      notify(e instanceof Error ? e.message : "failed to create client");
    }
  }

  async function toggle(c: Client) {
    try {
      await send(`/admin/clients/${c.id}`, "POST", { disabled: c.disabled === 0 });
      await reload();
      onChanged();
    } catch (e) {
      notify(e instanceof Error ? e.message : "failed to update client");
    }
  }

  async function remove(c: Client) {
    try {
      await send(`/admin/clients/${c.id}`, "DELETE");
      await reload();
      if (selectedId === c.id) onSelect(null);
      onChanged();
    } catch (e) {
      notify(e instanceof Error ? e.message : "failed to delete client");
    }
  }

  return (
    <section class="sec">
      <h2># clients</h2>
      <form class="form sec-form" onSubmit={create}>
        <span class="field-label">New client name</span>
        <div class="input-row">
          <input
            class="inp"
            value={name}
            onInput={(e) => setName((e.target as HTMLInputElement).value)}
            placeholder="laptop-dev"
          />
          <button class="btn btn-accent" type="submit">
            create
          </button>
        </div>
        <span class="field-hint">a human label for this agent (e.g. laptop-dev)</span>
      </form>

      <div class="sec-list">
        {clients.map((c) => {
          const isOnline = online[c.id] ?? false;
          return (
            <div class={`li${selectedId === c.id ? " selected" : ""}`} key={c.id}>
              <button
                class="li-main"
                onClick={() => onSelect(c.id)}
                title={`${c.name} (${isOnline ? "connected" : "offline"})`}
              >
                <span class={isOnline ? "dot accent" : "dot muted"}>{isOnline ? "●" : "○"}</span>
                <span class="li-name">{c.name}</span>
                <span class="muted li-prefix">{c.token_prefix}…</span>
                {c.disabled ? <span class="chip warn">disabled</span> : null}
              </button>
              <span class="li-actions">
                <button class="btn" onClick={() => toggle(c)}>
                  {c.disabled ? "enable" : "disable"}
                </button>
                <button class="btn" onClick={() => setPendingDelete(c)}>
                  delete
                </button>
              </span>
            </div>
          );
        })}
      </div>

      {newToken && <TokenDialog token={newToken} onDismiss={() => setNewToken(null)} />}
      {pendingDelete && (
        <ConfirmDialog
          message={`delete client "${pendingDelete.name}"? routes referencing it will stop resolving.`}
          confirmLabel="delete"
          onConfirm={() => remove(pendingDelete)}
          onClose={() => setPendingDelete(null)}
        />
      )}
    </section>
  );
}
