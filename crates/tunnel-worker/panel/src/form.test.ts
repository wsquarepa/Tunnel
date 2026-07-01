import { describe, it, expect } from "vitest";
import { matcherHint, matcherPlaceholder } from "./form";

describe("matcher helpers", () => {
  it("describes path routes as a URL slug", () => {
    expect(matcherHint("path")).toContain("slug");
    expect(matcherPlaceholder("path")).toBe("api");
  });
  it("describes subdomain routes as a label", () => {
    expect(matcherHint("subdomain")).toContain("subdomain");
    expect(matcherPlaceholder("subdomain")).toBe("docs");
  });
});
