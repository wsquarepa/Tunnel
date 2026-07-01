import { useEffect, useState } from "preact/hooks";
import { getJson, send } from "../api";
import type { Client, CreatedClient } from "../types";
import { Field } from "./Field";
import { TokenToast } from "./Toast";

interface ClientsProps {
  selectedId: string | null;
  onSelect: (id: string) => void;
  onChanged: () => void;
}

export function Clients({ selectedId, onSelect, onChanged }: ClientsProps) {
  const [clients, setClients] = useState<Client[]>([]);
  const [name, setName] = useState("");
  const [newToken, setNewToken] = useState<string | null>(null);

  async function load() {
    setClients(await getJson<Client[]>("/admin/clients"));
  }
  useEffect(() => {
    void load();
  }, []);

  async function create(e: Event) {
    e.preventDefault();
    if (!name.trim()) return;
    const created = await send<CreatedClient>("/admin/clients", "POST", { name });
    setNewToken(created.token);
    setName("");
    await load();
    onChanged();
  }

  async function toggle(c: Client) {
    await send(`/admin/clients/${c.id}`, "POST", { disabled: c.disabled === 0 });
    await load();
    onChanged();
  }

  async function remove(c: Client) {
    if (!confirm(`delete client "${c.name}"? routes referencing it will stop resolving.`)) return;
    await send(`/admin/clients/${c.id}`, "DELETE");
    await load();
    onChanged();
  }

  return (
    <section class="sec">
      <h2># clients</h2>
      {newToken && <TokenToast token={newToken} onDismiss={() => setNewToken(null)} />}
      {clients.map((c) => (
        <div class="li" key={c.id}>
          <span>
            <span class={c.disabled ? "muted" : "accent"}>{c.disabled ? "○" : "●"}</span>{" "}
            <button class="btn" onClick={() => onSelect(c.id)}>
              {c.name}
            </button>{" "}
            <span class="muted">{c.token_prefix}…</span>
            {c.disabled ? <span class="warn"> disabled</span> : null}
            {selectedId === c.id ? <span class="accent"> ◂ status</span> : null}
          </span>
          <span>
            <button class="btn" onClick={() => toggle(c)}>
              {c.disabled ? "enable" : "disable"}
            </button>{" "}
            <button class="btn" onClick={() => remove(c)}>
              delete
            </button>
          </span>
        </div>
      ))}
      <form class="row-form" onSubmit={create}>
        <Field label="New client name" hint="a human label for this agent, e.g. laptop-dev">
          <input
            class="inp"
            value={name}
            onInput={(e) => setName((e.target as HTMLInputElement).value)}
            placeholder="laptop-dev"
          />
        </Field>
        <button class="btn btn-accent" type="submit">
          create
        </button>
      </form>
    </section>
  );
}
