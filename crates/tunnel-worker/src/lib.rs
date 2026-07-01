mod admin;
pub mod auth;
pub mod routing;
mod session;
pub mod session_helpers;
pub mod store;
mod token;

pub use session::TunnelSession;

use worker::*;

#[event(fetch, respond_with_errors)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    Router::new()
        .on_async(
            "/admin",
            |req, ctx| async move { admin::handle(req, ctx).await },
        )
        // Match the bare `/admin/` too: the `/admin/*rest` glob does not match an
        // empty rest, so without this it falls through to the public tunnel 404.
        .on_async("/admin/", |req, ctx| async move {
            admin::handle(req, ctx).await
        })
        .on_async("/admin/*rest", |req, ctx| async move {
            admin::handle(req, ctx).await
        })
        .on_async("/_tunnel/connect", |req, ctx| async move {
            crate::session::route_connect(req, ctx).await
        })
        // Root has no slug; route it through the public handler so it returns the
        // consistent `no such tunnel` 404 instead of the Router's default body.
        .or_else_any_method_async("/", |req, ctx| async move {
            crate::session::route_public(req, ctx).await
        })
        .or_else_any_method_async("/*rest", |req, ctx| async move {
            crate::session::route_public(req, ctx).await
        })
        .run(req, env)
        .await
}
