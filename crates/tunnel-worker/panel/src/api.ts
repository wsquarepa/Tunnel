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
  // Admin mutations reply with either JSON (create client/route) or a plain-text
  // acknowledgement ("ok"/"deleted"/"enabled"). Only parse when it is actually
  // JSON — `JSON.parse("ok")` would throw and abort an otherwise-successful call
  // (e.g. login, whose 200 body is "ok").
  const contentType = res.headers.get("content-type") ?? "";
  if (contentType.includes("application/json")) {
    return (await res.json()) as T;
  }
  return undefined as T;
}
