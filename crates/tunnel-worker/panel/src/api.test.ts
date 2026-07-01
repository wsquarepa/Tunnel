import { describe, it, expect, vi, afterEach } from "vitest";
import { requestInit, send } from "./api";

function stubFetch(status: number, body: string, contentType: string | null): void {
  const headers = new Headers();
  if (contentType !== null) headers.set("content-type", contentType);
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response(body, { status, headers })),
  );
}

describe("requestInit", () => {
  it("sets CSRF header and same-origin credentials", () => {
    const init = requestInit({ method: "POST" });
    expect(init.credentials).toBe("same-origin");
    const h = init.headers as Record<string, string>;
    expect(h["X-Tunnel-CSRF"]).toBe("1");
    expect(h["Content-Type"]).toBe("application/json");
    expect(init.method).toBe("POST");
  });

  it("keeps caller headers alongside the CSRF header", () => {
    const init = requestInit({ headers: { "X-Extra": "y" } });
    const h = init.headers as Record<string, string>;
    expect(h["X-Extra"]).toBe("y");
    expect(h["X-Tunnel-CSRF"]).toBe("1");
  });
});

describe("send", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("resolves on a plain-text OK body without trying to parse it as JSON", async () => {
    // Regression: login returns 200 with body "ok" and no JSON content-type;
    // JSON.parse("ok") used to throw and surface as a bogus "login failed".
    stubFetch(200, "ok", "text/plain");
    await expect(send("/admin/login", "POST", { secret: "test" })).resolves.toBeUndefined();
  });

  it("parses the body when the response is JSON", async () => {
    stubFetch(200, JSON.stringify({ token: "tnl_abc" }), "application/json");
    await expect(send<{ token: string }>("/admin/clients", "POST", { name: "x" })).resolves.toEqual({
      token: "tnl_abc",
    });
  });

  it("throws with status context on a non-ok response", async () => {
    stubFetch(403, "missing CSRF header", "text/plain");
    await expect(send("/admin/clients", "POST", {})).rejects.toThrow("403");
  });
});
