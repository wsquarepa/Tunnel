import { useEffect, useState } from "preact/hooks";
import { getJson, send } from "../api";
import type { Client, Route, RouteKind } from "../types";
import { Field } from "./Field";
import { matcherHint, matcherPlaceholder } from "../form";
import { notify } from "../toast";

interface RoutesProps {
  changeTick: number;
}

export function Routes({ changeTick }: RoutesProps) {
  const [routes, setRoutes] = useState<Route[]>([]);
  const [clients, setClients] = useState<Client[]>([]);
  const [clientId, setClientId] = useState("");
  const [kind, setKind] = useState<RouteKind>("path");
  const [matcher, setMatcher] = useState("");
  const [target, setTarget] = useState("");

  async function load() {
    const [r, c] = await Promise.all([
      getJson<Route[]>("/admin/routes"),
      getJson<Client[]>("/admin/clients"),
    ]);
    setRoutes(r);
    setClients(c);
    // Keep the selection valid: default to the first client, and reset if the
    // chosen client was deleted elsewhere.
    if (c.length > 0 && !c.some((x) => x.id === clientId)) setClientId(c[0].id);
  }
  useEffect(() => {
    void load();
  }, [changeTick]);

  async function create(e: Event) {
    e.preventDefault();
    try {
      await send("/admin/routes", "POST", { client_id: clientId, kind, matcher, target });
      setMatcher("");
      setTarget("");
      await load();
    } catch (err) {
      notify(err instanceof Error ? err.message : "failed to add route");
    }
  }

  async function remove(r: Route) {
    try {
      await send(`/admin/routes/${r.id}`, "DELETE");
      await load();
    } catch (err) {
      notify(err instanceof Error ? err.message : "failed to remove route");
    }
  }

  const clientName = (id: string) => clients.find((c) => c.id === id)?.name ?? id;

  return (
    <section class="sec">
      <h2># routes</h2>
      <form class="form sec-form" onSubmit={create}>
        <Field label="Client" hint="the agent that will serve this route">
          <select
            class="inp"
            value={clientId}
            onChange={(e) => setClientId((e.target as HTMLSelectElement).value)}
          >
            {clients.map((c) => (
              <option value={c.id} key={c.id}>
                {c.name}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Kind" hint="path prefix or wildcard subdomain">
          <select
            class="inp"
            value={kind}
            onChange={(e) => setKind((e.target as HTMLSelectElement).value as RouteKind)}
          >
            <option value="path">path</option>
            <option value="subdomain">subdomain</option>
          </select>
        </Field>
        <Field label="Matcher" hint={matcherHint(kind)}>
          <input
            class="inp"
            value={matcher}
            onInput={(e) => setMatcher((e.target as HTMLInputElement).value)}
            placeholder={matcherPlaceholder(kind)}
          />
        </Field>
        <Field label="Target" hint="a target name from that client's allowlist (e.g. web)">
          <input
            class="inp"
            value={target}
            onInput={(e) => setTarget((e.target as HTMLInputElement).value)}
            placeholder="web"
          />
        </Field>
        <div class="form-actions">
          <button class="btn btn-accent" type="submit" disabled={!clientId}>
            add
          </button>
        </div>
      </form>

      <div class="sec-list">
        {routes.map((r) => (
          <div class="li" key={r.id}>
            <span class="li-main" title={`${r.matcher} to ${r.target}`}>
              <span class="accent">{r.kind === "path" ? "path" : "sub"}</span>{" "}
              <span class="li-name">
                {r.matcher} &rarr; <b>{r.target}</b>{" "}
                <span class="muted">({clientName(r.client_id)})</span>
              </span>
            </span>
            <span class="li-actions">
              <button class="btn" onClick={() => remove(r)}>
                delete
              </button>
            </span>
          </div>
        ))}
      </div>
    </section>
  );
}
