use serde::{Deserialize, Serialize};
use worker::*;

use crate::{auth, routing, store, token};

const COOKIE_NAME: &str = "tunnel_session";
const MAX_AGE_SECS: i64 = 12 * 3600;
/// Custom header the panel sends on every mutation. A cross-origin attacker
/// cannot set it on a form post or simple request, so requiring it blocks
/// CROSS-origin CSRF. It does NOT defend against a malicious same-origin
/// path-mode tenant (covered by the README [!CAUTION] / separate-origin advice).
const CSRF_HEADER: &str = "X-Tunnel-CSRF";

fn now_secs() -> i64 {
    (Date::now().as_millis() / 1000) as i64
}

fn unauthorized() -> Result<Response> {
    Response::error("unauthorized", 401)
}

fn require_session(req: &Request, secret: &str) -> bool {
    let Ok(Some(cookie_header)) = req.headers().get("Cookie") else {
        return false;
    };
    cookie_header
        .split(';')
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == COOKIE_NAME)
        .map(|(_, v)| auth::verify_session(secret, v, now_secs(), MAX_AGE_SECS))
        .unwrap_or(false)
}

#[derive(Deserialize)]
struct LoginBody {
    secret: String,
}

#[derive(Deserialize)]
struct CreateClientBody {
    name: String,
}

#[derive(Serialize)]
struct CreatedClient {
    id: String,
    name: String,
    token: String,
    token_prefix: String,
}

#[derive(Deserialize)]
struct CreateRouteBody {
    client_id: String,
    kind: String,
    matcher: String,
    target: String,
}

#[derive(Deserialize)]
struct SetDisabledBody {
    disabled: bool,
}

pub async fn handle(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let secret = ctx.secret("ADMIN_SECRET")?.to_string();
    let db = ctx.env.d1("DB")?;
    let path = req.path();
    let method = req.method();

    // Serve the static panel (login page + its JS/CSS) without a session so the
    // login UI can load. Only the JSON API endpoints below are session-gated.
    if method == Method::Get
        && (path == "/admin"
            || path == "/admin/"
            || path.ends_with(".js")
            || path.ends_with(".css")
            || path.ends_with(".html"))
    {
        let assets = ctx.env.assets("ASSETS")?;
        let file = if path == "/admin" || path == "/admin/" {
            "/index.html".to_string()
        } else {
            path.strip_prefix("/admin").unwrap_or(&path).to_string()
        };
        return assets.fetch(format!("http://assets{file}"), None).await;
    }

    // Login is the only unauthenticated endpoint.
    if path == "/admin/login" && method == Method::Post {
        let Ok(body) = req.json::<LoginBody>().await else {
            return Response::error("invalid request body", 400);
        };
        if !auth::constant_time_eq(body.secret.as_bytes(), secret.as_bytes()) {
            return unauthorized();
        }
        let cookie = auth::sign_session(&secret, now_secs());
        let headers = Headers::new();
        headers.set(
            "Set-Cookie",
            &format!(
                "{COOKIE_NAME}={cookie}; HttpOnly; Secure; SameSite=Strict; Path=/admin; Max-Age={MAX_AGE_SECS}"
            ),
        )?;
        return Ok(Response::ok("ok")?.with_headers(headers));
    }

    if !require_session(&req, &secret) {
        return unauthorized();
    }

    // Mutations require the panel's custom header (cross-origin CSRF defense).
    // GET reads (list/status/assets) and login are intentionally exempt.
    if matches!(method, Method::Post | Method::Delete) && req.headers().get(CSRF_HEADER)?.is_none()
    {
        return Response::error("missing CSRF header", 403);
    }

    match (method.clone(), path.as_str()) {
        (Method::Get, p) if p.starts_with("/admin/clients/") && p.ends_with("/status") => {
            let id = &p["/admin/clients/".len()..p.len() - "/status".len()];
            let stub = ctx.durable_object("TUNNEL")?.id_from_name(id)?.get_stub()?;
            stub.fetch_with_str("http://do/status").await
        }
        (Method::Get, "/admin/clients") => {
            let clients = store::list_clients(&db).await?;
            Response::from_json(&clients)
        }
        (Method::Post, "/admin/clients") => {
            let Ok(body) = req.json::<CreateClientBody>().await else {
                return Response::error("invalid request body", 400);
            };
            if body.name.trim().is_empty() {
                return Response::error("name is required", 400);
            }
            let (tok, hash, prefix) = token::generate();
            let row = store::ClientRow {
                id: token::sha256_hex(&format!("id{}{}", now_secs(), prefix))[..26].to_string(),
                name: body.name.clone(),
                token_hash: hash,
                token_prefix: prefix.clone(),
                created_at: now_secs(),
                disabled: 0,
            };
            match store::insert_client(&db, &row).await {
                // D1 surfaces the SQLite UNIQUE violation (client name) as an error
                // whose text contains "UNIQUE"; map it to a clean 409 rather than
                // letting the raw D1Error (with a JS stack trace) reach the client.
                Ok(()) => Response::from_json(&CreatedClient {
                    id: row.id,
                    name: body.name,
                    token: tok,
                    token_prefix: prefix,
                }),
                Err(e) if e.to_string().contains("UNIQUE") => {
                    Response::error("a client with this name already exists", 409)
                }
                Err(_) => Response::error("failed to create client", 500),
            }
        }
        (Method::Get, "/admin/routes") => {
            let routes = store::list_routes(&db).await?;
            Response::from_json(&routes)
        }
        (Method::Post, "/admin/routes") => {
            let Ok(body) = req.json::<CreateRouteBody>().await else {
                return Response::error("invalid request body", 400);
            };
            if body.kind != "path" && body.kind != "subdomain" {
                return Response::error("kind must be 'path' or 'subdomain'", 400);
            }
            if body.kind == "path" && routing::is_reserved_slug(&body.matcher) {
                return Response::error("matcher is reserved", 400);
            }
            let row = store::RouteRow {
                id: token::sha256_hex(&format!("rt{}{}", now_secs(), body.matcher))[..26]
                    .to_string(),
                client_id: body.client_id,
                kind: body.kind,
                matcher: body.matcher,
                target: body.target,
                strip_prefix: 1,
                created_at: now_secs(),
            };
            match store::insert_route(&db, &row).await {
                Ok(()) => Response::from_json(&row),
                // D1 surfaces the SQLite `UNIQUE(kind,matcher)` violation as an error
                // whose text contains "UNIQUE"; map it to a clean 409 rather than
                // letting the raw D1Error (with a JS stack trace) reach the client.
                Err(e) if e.to_string().contains("UNIQUE") => {
                    Response::error("a route with this matcher already exists", 409)
                }
                Err(_) => Response::error("failed to create route", 500),
            }
        }
        (Method::Post, "/admin/logout") => {
            let headers = Headers::new();
            headers.set(
                "Set-Cookie",
                &format!(
                    "{COOKIE_NAME}=; HttpOnly; Secure; SameSite=Strict; Path=/admin; Max-Age=0"
                ),
            )?;
            Ok(Response::ok("ok")?.with_headers(headers))
        }
        (Method::Post, p) if p.starts_with("/admin/clients/") && !p.ends_with("/status") => {
            let id = p["/admin/clients/".len()..].to_string();
            // A missing/invalid body means "disable" (the pre-toggle behavior).
            let disabled = req
                .json::<SetDisabledBody>()
                .await
                .map(|b| b.disabled)
                .unwrap_or(true);
            store::set_client_disabled(&db, &id, disabled).await?;
            Response::ok(if disabled { "disabled" } else { "enabled" })
        }
        _ => handle_item(method, &path, &db).await,
    }
}

async fn handle_item(method: Method, path: &str, db: &D1Database) -> Result<Response> {
    if let Some(id) = path.strip_prefix("/admin/clients/") {
        if method == Method::Delete {
            store::delete_client(db, id).await?;
            return Response::ok("deleted");
        }
    }
    if let Some(id) = path.strip_prefix("/admin/routes/") {
        if method == Method::Delete {
            store::delete_route(db, id).await?;
            return Response::ok("deleted");
        }
    }
    Response::error("not found", 404)
}
