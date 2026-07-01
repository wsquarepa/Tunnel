import { useEffect, useState } from "preact/hooks";
import { getJson, send } from "../api";
import type { Client, CreatedClient, Status } from "../types";
import { notify } from "../toast";
import { TokenDialog } from "./Toast";
import { ConfirmDialog } from "./ConfirmDialog";

interface ClientsProps {
  selectedId: string | null;
  onSelect: (id: string | null) => void;
  onChanged: () => void;
}

const REFRESH_MS = 5000;

// The list dot shows real connection state, so it must poll each client's
// Durable Object status (there is no aggregate registry of live sockets). That
// is one request per client per refresh; fine for the handful of clients a
// self-hosted tunnel has. If a deployment ever grows to many clients, add a
// server-side aggregate endpoint instead of fanning out here.
async function fetchState(): Promise<{ list: Client[]; online: Record<string, boolean> }> {
  const list = await getJson<Client[]>("/admin/clients");
  const entries = await Promise.all(
    list.map(async (c) => {
      try {
        const s = await getJson<Status>(`/admin/clients/${c.id}/status`);
        return [c.id, (s.connections ?? 0) > 0] as const;
      } catch {
        return [c.id, false] as const;
      }
    }),
  );
  return { list, online: Object.fromEntries(entries) };
}

export function Clients({ selectedId, onSelect, onChanged }: ClientsProps) {
  const [clients, setClients] = useState<Client[]>([]);
  const [online, setOnline] = useState<Record<string, boolean>>({});
  const [name, setName] = useState("");
  const [newToken, setNewToken] = useState<string | null>(null);
  const [pendingDelete, setPendingDelete] = useState<Client | null>(null);

  async function reload() {
    const { list, online: conn } = await fetchState();
    setClients(list);
    setOnline(conn);
  }

  useEffect(() => {
    let live = true;
    async function refresh() {
      try {
        const { list, online: conn } = await fetchState();
        if (live) {
          setClients(list);
          setOnline(conn);
        }
      } catch {
        // Leave the last-known state on a transient fetch failure.
      }
    }
    void refresh();
    const id = setInterval(refresh, REFRESH_MS);
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
