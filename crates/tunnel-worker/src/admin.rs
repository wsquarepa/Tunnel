use serde::{Deserialize, Serialize};
use worker::*;

use crate::{auth, routing, store, token};

const COOKIE_NAME: &str = "tunnel_session";
const MAX_AGE_SECS: i64 = 12 * 3600;

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

pub async fn handle(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let secret = ctx.secret("ADMIN_SECRET")?.to_string();
    let db = ctx.env.d1("DB")?;
    let path = req.path();
    let method = req.method();

    // Login is the only unauthenticated endpoint.
    if path == "/admin/login" && method == Method::Post {
        let body: LoginBody = req.json().await?;
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

    match (method.clone(), path.as_str()) {
        (Method::Get, "/admin/clients") => {
            let clients = store::list_clients(&db).await?;
            Response::from_json(&clients)
        }
        (Method::Post, "/admin/clients") => {
            let body: CreateClientBody = req.json().await?;
            let (tok, hash, prefix) = token::generate();
            let row = store::ClientRow {
                id: token::sha256_hex(&format!("id{}{}", now_secs(), prefix))[..26].to_string(),
                name: body.name.clone(),
                token_hash: hash,
                token_prefix: prefix.clone(),
                created_at: now_secs(),
                disabled: 0,
            };
            store::insert_client(&db, &row).await?;
            Response::from_json(&CreatedClient {
                id: row.id,
                name: body.name,
                token: tok,
                token_prefix: prefix,
            })
        }
        (Method::Get, "/admin/routes") => {
            let routes = store::list_routes(&db).await?;
            Response::from_json(&routes)
        }
        (Method::Post, "/admin/routes") => {
            let body: CreateRouteBody = req.json().await?;
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
            store::insert_route(&db, &row).await?;
            Response::from_json(&row)
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
        _ => handle_item(method, &path, &db).await,
    }
}

async fn handle_item(method: Method, path: &str, db: &D1Database) -> Result<Response> {
    if let Some(id) = path.strip_prefix("/admin/clients/") {
        match method {
            Method::Delete => {
                store::delete_client(db, id).await?;
                return Response::ok("deleted");
            }
            Method::Post => {
                store::set_client_disabled(db, id, true).await?;
                return Response::ok("disabled");
            }
            _ => {}
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
