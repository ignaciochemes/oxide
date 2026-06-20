//! Servidor admin / dashboard + panel de control.
//!
//! En su propio puerto (default 9090) expone:
//!   - `GET  /status`         -> snapshot del estado en JSON
//!   - `GET  /ws`             -> WebSocket con eventos en vivo
//!   - `GET  /api/config`     -> config actual (backends, algoritmo, rutas)
//!   - `POST /api/backends`   -> agrega un backend            (escritura)
//!   - `DELETE /api/backends` -> quita un backend             (escritura)
//!   - `PUT  /api/algorithm`  -> cambia el algoritmo          (escritura)
//!
//! Las escrituras editan el `config.toml`; la recarga en caliente las aplica.
//! Si `[admin] token` está configurado, las escrituras requieren
//! `Authorization: Bearer <token>`.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_tungstenite::tungstenite::Message;
use hyper_util::rt::TokioIo;
use serde::Deserialize;
use tokio::net::TcpListener;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::watch;

use crate::configfile;
use crate::events::{Event, EventTx};
use crate::proxy::snapshot_backends;
use crate::router::SharedRouter;

/// Estado compartido del servidor admin.
struct Ctx {
    router: SharedRouter,
    events: EventTx,
    config_path: String,
    token: Option<String>,
}

pub async fn run(
    addr: SocketAddr,
    router: SharedRouter,
    events: EventTx,
    config_path: String,
    token: Option<String>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("Admin/dashboard en http://{addr}  (WebSocket en ws://{addr}/ws)");

    let ctx = Arc::new(Ctx {
        router,
        events,
        config_path,
        token,
    });

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, _peer) = accepted?;
                let io = TokioIo::new(stream);
                let ctx = ctx.clone();
                tokio::spawn(async move {
                    let service = service_fn(move |req| handle(req, ctx.clone()));
                    let conn = http1::Builder::new()
                        .serve_connection(io, service)
                        .with_upgrades();
                    if let Err(err) = conn.await {
                        tracing::debug!("conexión admin terminó con error: {err:?}");
                    }
                });
            }
            _ = shutdown.changed() => {
                tracing::info!("admin: dejando de aceptar conexiones");
                break;
            }
        }
    }

    Ok(())
}

async fn handle(
    mut req: Request<Incoming>,
    ctx: Arc<Ctx>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let path = req.uri().path().to_string();

    // WebSocket: no necesita CORS (el browser no hace preflight de WS).
    if path == "/ws" && hyper_tungstenite::is_upgrade_request(&req) {
        return match hyper_tungstenite::upgrade(&mut req, None) {
            Ok((response, websocket)) => {
                let rx = ctx.events.subscribe();
                let router = ctx.router.clone();
                tokio::spawn(serve_ws(websocket, router, rx));
                Ok(response)
            }
            Err(err) => Ok(with_cors(text(
                StatusCode::BAD_REQUEST,
                format!("error en el upgrade de WebSocket: {err}"),
            ))),
        };
    }

    let method = req.method().clone();

    // Preflight CORS para las llamadas del navegador (POST/PUT/DELETE con JSON).
    if method == Method::OPTIONS {
        return Ok(with_cors(empty(StatusCode::NO_CONTENT)));
    }

    let resp = route(method, path, req, ctx).await;
    Ok(with_cors(resp))
}

/// Rutea cada request del admin a su handler.
async fn route(
    method: Method,
    path: String,
    req: Request<Incoming>,
    ctx: Arc<Ctx>,
) -> Response<Full<Bytes>> {
    match (&method, path.as_str()) {
        (&Method::GET, "/status") => json(StatusCode::OK, &snapshot_json(&ctx)),

        (&Method::GET, "/api/config") => json(StatusCode::OK, &config_json(&ctx)),

        (&Method::POST, "/api/backends") => {
            guard(&ctx, req, |body| {
                let dto: AddBackend = parse(body)?;
                configfile::add_backend(&ctx.config_path, &dto.url, dto.weight.unwrap_or(1))
            })
            .await
        }

        (&Method::DELETE, "/api/backends") => {
            guard(&ctx, req, |body| {
                let dto: RemoveBackend = parse(body)?;
                configfile::remove_backend(&ctx.config_path, &dto.url)
            })
            .await
        }

        (&Method::PUT, "/api/algorithm") => {
            guard(&ctx, req, |body| {
                let dto: SetAlgorithm = parse(body)?;
                configfile::set_algorithm(&ctx.config_path, &dto.algorithm)
            })
            .await
        }

        _ => text(StatusCode::NOT_FOUND, "not found".to_string()),
    }
}

// --- DTOs de las requests de escritura ---

#[derive(Deserialize)]
struct AddBackend {
    url: String,
    weight: Option<u32>,
}
#[derive(Deserialize)]
struct RemoveBackend {
    url: String,
}
#[derive(Deserialize)]
struct SetAlgorithm {
    algorithm: String,
}

fn parse<T: for<'de> Deserialize<'de>>(body: &[u8]) -> anyhow::Result<T> {
    serde_json::from_slice(body).map_err(|e| anyhow::anyhow!("JSON inválido: {e}"))
}

/// Chequea auth, lee el body y ejecuta la acción de escritura. Devuelve un JSON
/// `{ok:true}` o `{error:...}` con el status adecuado.
async fn guard<F>(ctx: &Ctx, req: Request<Incoming>, action: F) -> Response<Full<Bytes>>
where
    F: FnOnce(&[u8]) -> anyhow::Result<()>,
{
    if !authorized(&req, ctx) {
        return json(
            StatusCode::UNAUTHORIZED,
            &serde_json::json!({ "error": "token inválido o faltante" }),
        );
    }

    let body = match req.into_body().collect().await {
        Ok(c) => c.to_bytes(),
        Err(e) => {
            return json(
                StatusCode::BAD_REQUEST,
                &serde_json::json!({ "error": format!("no pude leer el body: {e}") }),
            )
        }
    };

    match action(&body) {
        Ok(()) => json(StatusCode::OK, &serde_json::json!({ "ok": true })),
        Err(e) => json(
            StatusCode::BAD_REQUEST,
            &serde_json::json!({ "error": format!("{e}") }),
        ),
    }
}

/// `true` si no hay token configurado, o si el header coincide.
fn authorized(req: &Request<Incoming>, ctx: &Ctx) -> bool {
    let Some(expected) = &ctx.token else {
        return true;
    };
    req.headers()
        .get(hyper::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|h| h.trim_start_matches("Bearer ").trim() == expected)
        .unwrap_or(false)
}

fn snapshot_json(ctx: &Ctx) -> serde_json::Value {
    let current = ctx.router.load_full();
    let backends = snapshot_backends(&current);
    let total: u64 = backends.iter().map(|b| b.requests).sum();
    serde_json::json!({ "service": "oxide", "total_requests": total, "backends": backends })
}

/// Config actual (lo que el panel de control necesita para renderizar el form).
/// La leemos del archivo para reflejar exactamente lo persistido.
fn config_json(ctx: &Ctx) -> serde_json::Value {
    match crate::config::Config::load(&ctx.config_path) {
        Ok(cfg) => {
            let backends: Vec<_> = cfg
                .upstreams
                .iter()
                .map(|u| serde_json::json!({ "url": u.url, "weight": u.weight }))
                .collect();
            // URL del proxy (para el botón "Probar tráfico" del dashboard).
            let scheme = if cfg.tls.enabled { "https" } else { "http" };
            let host = cfg.listen.replace("0.0.0.0", "127.0.0.1");
            serde_json::json!({
                "algorithm": cfg.balancer.algorithm,
                "backends": backends,
                "proxy_url": format!("{scheme}://{host}"),
            })
        }
        Err(e) => serde_json::json!({ "error": format!("{e}") }),
    }
}

async fn serve_ws(
    websocket: hyper_tungstenite::HyperWebsocket,
    router: SharedRouter,
    mut rx: tokio::sync::broadcast::Receiver<Event>,
) {
    let ws = match websocket.await {
        Ok(ws) => ws,
        Err(err) => {
            tracing::debug!("no se completó el WebSocket: {err}");
            return;
        }
    };
    let (mut sink, mut stream) = ws.split();

    // Snapshot inicial.
    let current = router.load_full();
    let backends = snapshot_backends(&current);
    let total: u64 = backends.iter().map(|b| b.requests).sum();
    let snap = Event::Snapshot {
        backends,
        total_requests: total,
    };
    if let Ok(txt) = serde_json::to_string(&snap) {
        if sink.send(Message::Text(txt.into())).await.is_err() {
            return;
        }
    }

    loop {
        tokio::select! {
            event = rx.recv() => match event {
                Ok(event) => {
                    if let Ok(txt) = serde_json::to_string(&event) {
                        if sink.send(Message::Text(txt.into())).await.is_err() {
                            break;
                        }
                    }
                }
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => break,
            },
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(_)) => {}
                Some(Err(_)) => break,
            },
        }
    }
}

// --- Helpers de respuesta ---

fn with_cors(mut resp: Response<Full<Bytes>>) -> Response<Full<Bytes>> {
    let h = resp.headers_mut();
    h.insert("access-control-allow-origin", "*".parse().unwrap());
    h.insert(
        "access-control-allow-methods",
        "GET, POST, PUT, DELETE, OPTIONS".parse().unwrap(),
    );
    h.insert(
        "access-control-allow-headers",
        "content-type, authorization".parse().unwrap(),
    );
    resp
}

fn json(status: StatusCode, value: &serde_json::Value) -> Response<Full<Bytes>> {
    let text = serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string());
    let mut resp = Response::new(Full::new(Bytes::from(text)));
    *resp.status_mut() = status;
    resp.headers_mut()
        .insert("content-type", "application/json".parse().unwrap());
    resp
}

fn text(status: StatusCode, body: String) -> Response<Full<Bytes>> {
    let mut resp = Response::new(Full::new(Bytes::from(body)));
    *resp.status_mut() = status;
    resp.headers_mut()
        .insert("content-type", "text/plain; charset=utf-8".parse().unwrap());
    resp
}

fn empty(status: StatusCode) -> Response<Full<Bytes>> {
    let mut resp = Response::new(Full::new(Bytes::new()));
    *resp.status_mut() = status;
    resp
}
