import { describe, it, expect } from "vitest";
import { requestInit } from "./api";

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
