export type RouteKind = "path" | "subdomain";

export interface Client {
  id: string;
  name: string;
  token_prefix: string;
  disabled: number;
  created_at: number;
}

export interface CreatedClient {
  id: string;
  name: string;
  token: string;
  token_prefix: string;
}

export interface Route {
  id: string;
  client_id: string;
  kind: RouteKind;
  matcher: string;
  target: string;
  strip_prefix: number;
  created_at: number;
}

export interface RequestLogRow {
  ts: number;
  method: string;
  path: string;
  status: number;
  latency_ms: number;
  target: string;
}

export interface PoolSocket {
  id: number;
  connected_at: number;
  active_streams: number;
}

export interface Status {
  connections: number;
  last_seen: number;
  sockets: PoolSocket[];
  recent: RequestLogRow[];
}
