// Every request carries X-Tunnel-CSRF; the Worker rejects mutations without it
// as a cross-origin CSRF defense. Sending it on GETs too is harmless.
export function requestInit(opts: RequestInit): RequestInit {
  return {
    credentials: "same-origin",
    ...opts,
    headers: {
      "Content-Type": "application/json",
      "X-Tunnel-CSRF": "1",
      ...(opts.headers ?? {}),
    },
  };
}

export async function getJson<T>(path: string): Promise<T> {
  const res = await fetch(path, requestInit({ method: "GET" }));
  if (!res.ok) throw new Error(`${res.status}: ${await res.text()}`);
  return (await res.json()) as T;
}

export async function send<T = unknown>(
  path: string,
  method: "POST" | "DELETE",
  body?: unknown,
): Promise<T> {
  const res = await fetch(
    path,
    requestInit({ method, body: body === undefined ? undefined : JSON.stringify(body) }),
  );
  if (!res.ok) throw new Error(`${res.status}: ${await res.text()}`);
  const text = await res.text();
  return (text ? JSON.parse(text) : undefined) as T;
}
