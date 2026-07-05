import { useEffect, useState } from "preact/hooks";
import { getJson } from "../api";
import type { PoolSocket, RequestLogRow, Status } from "../types";

interface ActivityProps {
  clientId: string;
}

const REFRESH_MS = 5000;

function statusClass(status: number): string {
  if (status >= 500) return "err";
  if (status >= 400) return "warn";
  return "accent";
}

function clock(ts: number): string {
  if (!ts) return "n/a";
  return new Date(ts).toLocaleTimeString();
}

export function Activity({ clientId }: ActivityProps) {
  const [status, setStatus] = useState<Status | null>(null);

  useEffect(() => {
    let live = true;
    async function tick() {
      try {
        const s = await getJson<Status>(`/admin/clients/${clientId}/status`);
        if (live) setStatus(s);
      } catch {
        if (live) setStatus(null);
      }
    }
    void tick();
    const id = setInterval(tick, REFRESH_MS);
    return () => {
      live = false;
      clearInterval(id);
    };
  }, [clientId]);

  const connected = (status?.connections ?? 0) > 0;

  return (
    <section class="sec activity">
      <h2>
        # activity{" "}
        <span class={connected ? "accent live-dot" : "muted"}>
          {connected ? "● live" : "○ offline"}
        </span>{" "}
        <span class="muted">
          {status
            ? `${status.connections} connection(s), last seen ${clock(status.last_seen)}`
            : "…"}
        </span>
      </h2>
      {(status?.sockets ?? []).map((s: PoolSocket) => (
        <p class="muted" key={s.id}>
          socket {s.id}: {s.active_streams} active stream(s), connected{" "}
          {new Date(s.connected_at).toLocaleTimeString()}
        </p>
      ))}
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>time</th>
              <th>method</th>
              <th>path</th>
              <th>status</th>
              <th>latency</th>
              <th>target</th>
            </tr>
          </thead>
          <tbody>
            {(status?.recent ?? []).map((r: RequestLogRow, i: number) => (
              <tr key={`${r.ts}-${i}`}>
                <td class="muted">{clock(r.ts)}</td>
                <td>{r.method}</td>
                <td class="cell-path">{r.path}</td>
                <td class={statusClass(r.status)}>{r.status}</td>
                <td>{r.latency_ms}ms</td>
                <td>{r.target}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}
