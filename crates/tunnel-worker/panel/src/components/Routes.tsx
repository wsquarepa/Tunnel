import { useEffect, useState } from "preact/hooks";
import { getJson, send } from "../api";
import type { Client, Route, RouteKind } from "../types";
import { Field } from "./Field";
import { matcherHint, matcherPlaceholder } from "../form";

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
  const [error, setError] = useState("");

  async function load() {
    const [r, c] = await Promise.all([
      getJson<Route[]>("/admin/routes"),
      getJson<Client[]>("/admin/clients"),
    ]);
    setRoutes(r);
    setClients(c);
    if (!clientId && c.length > 0) setClientId(c[0].id);
  }
  useEffect(() => {
    void load();
  }, [changeTick]);

  async function create(e: Event) {
    e.preventDefault();
    setError("");
    try {
      await send("/admin/routes", "POST", { client_id: clientId, kind, matcher, target });
      setMatcher("");
      setTarget("");
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "failed to add route");
    }
  }

  async function remove(r: Route) {
    await send(`/admin/routes/${r.id}`, "DELETE");
    await load();
  }

  const clientName = (id: string) => clients.find((c) => c.id === id)?.name ?? id;

  return (
    <section class="sec">
      <h2># routes</h2>
      {routes.map((r) => (
        <div class="li" key={r.id}>
          <span>
            <span class="accent">{r.kind === "path" ? "path" : "sub"}</span> {r.matcher} →{" "}
            <b>{r.target}</b> <span class="muted">({clientName(r.client_id)})</span>
          </span>
          <button class="btn" onClick={() => remove(r)}>
            delete
          </button>
        </div>
      ))}
      <form class="row-form" onSubmit={create}>
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
        <Field label="Target" hint="a target name from that client's allowlist, e.g. web">
          <input
            class="inp"
            value={target}
            onInput={(e) => setTarget((e.target as HTMLInputElement).value)}
            placeholder="web"
          />
        </Field>
        <button class="btn btn-accent" type="submit" disabled={!clientId}>
          add
        </button>
      </form>
      {error && <p class="err">{error}</p>}
    </section>
  );
}
