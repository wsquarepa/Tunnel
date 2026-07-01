import type { RouteKind } from "./types";

export function matcherHint(kind: RouteKind): string {
  return kind === "path"
    ? "URL slug, served at /<slug> (e.g. api)"
    : "subdomain label, served at <label>.<apex> (e.g. docs)";
}

export function matcherPlaceholder(kind: RouteKind): string {
  return kind === "path" ? "api" : "docs";
}
