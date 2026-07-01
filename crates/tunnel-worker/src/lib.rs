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
        // Spike route: forward any GET under /spike/* to the hard-coded DO instance.
        .get_async("/spike/*rest", |_req, ctx| async move {
            let stub = ctx
                .durable_object("TUNNEL")?
                .id_from_name("spike-client")?
                .get_stub()?;
            stub.fetch_with_str("http://do/req").await
        })
        .on_async("/admin", |req, ctx| async move {
            admin::handle(req, ctx).await
        })
        .on_async("/admin/*rest", |req, ctx| async move {
            admin::handle(req, ctx).await
        })
        .on_async("/_tunnel/connect", |req, ctx| async move {
            crate::session::route_connect(req, ctx).await
        })
        .or_else_any_method_async("/*rest", |req, ctx| async move {
            crate::session::route_public(req, ctx).await
        })
        .run(req, env)
        .await
}
