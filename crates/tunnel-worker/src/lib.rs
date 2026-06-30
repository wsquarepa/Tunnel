mod session;

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
        .on_async("/_tunnel/connect", |req, ctx| async move {
            // Spike: ignore auth, route every connect to the one DO instance.
            let stub = ctx
                .durable_object("TUNNEL")?
                .id_from_name("spike-client")?
                .get_stub()?;
            stub.fetch_with_request(req).await
        })
        .run(req, env)
        .await
}
