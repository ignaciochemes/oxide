//! El corazón del reverse proxy: recibe una request, elige la ruta (pool) según
//! host/path, elige un backend sano de ese pool (con el algoritmo configurado),
//! reenvía con timeout + reintentos y emite eventos para el dashboard.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::{Method, Request, Response, StatusCode, Uri};
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;

use crate::events::{self, BackendInfo, Event, EventTx};
use crate::router::Router;

pub type ProxyClient = Client<HttpConnector, Full<Bytes>>;
type ProxyBody = http_body_util::combinators::BoxBody<Bytes, hyper::Error>;

static REQUEST_SEQ: AtomicU64 = AtomicU64::new(1);

pub async fn handle(
    req: Request<Incoming>,
    router: Arc<Router>,
    client: ProxyClient,
    cfg: crate::config::ProxyConfig,
    events: EventTx,
    peer: SocketAddr,
) -> Result<Response<ProxyBody>, hyper::Error> {
    // Endpoint interno de estado: lo atiende Oxide mismo, no se proxea.
    if req.uri().path() == cfg.status_path {
        return Ok(status_response(&router));
    }

    let id = REQUEST_SEQ.fetch_add(1, Ordering::Relaxed);
    let start = Instant::now();
    let client_addr = peer.to_string();

    let (parts, body) = req.into_parts();
    let method = parts.method.to_string();
    let path = parts.uri.path().to_string();

    // Host (sin puerto) para el routing, tomado del header Host.
    let host = parts
        .headers
        .get(hyper::header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(|h| h.split(':').next().unwrap_or(h).to_string());

    // Elegimos el pool (ruta) según host + path.
    let balancer = router.select(host.as_deref(), &path).clone();
    let route_name = balancer.name().to_string();

    let body_bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(err) => {
            tracing::warn!("no pude leer el body del cliente: {err}");
            return Ok(error_response(StatusCode::BAD_REQUEST, "body inválido"));
        }
    };

    let path_and_query = parts
        .uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/")
        .to_string();

    let idempotent = matches!(
        parts.method,
        Method::GET | Method::HEAD | Method::PUT | Method::DELETE | Method::OPTIONS | Method::TRACE
    );
    let max_attempts = if idempotent { 1 + cfg.max_retries } else { 1 };

    let timeout = Duration::from_secs(cfg.request_timeout_secs);
    let mut last_status = StatusCode::BAD_GATEWAY;
    let mut last_backend = String::from("-");

    for attempt in 1..=max_attempts {
        let backend = match balancer.next_backend() {
            Some(backend) => backend,
            None => {
                tracing::error!("ruta '{route_name}': no hay backends sanos");
                emit_request(
                    &events, id, &method, &path, "(sin backend)", &route_name, 503, false,
                    attempt, start.elapsed(), &client_addr,
                );
                return Ok(error_response(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "no hay backends disponibles",
                ));
            }
        };

        let new_uri = match build_uri(&backend.uri, &path_and_query) {
            Ok(uri) => uri,
            Err(err) => {
                tracing::error!("URI inválida hacia {}: {err}", backend.uri);
                last_backend = backend.uri.to_string();
                continue;
            }
        };

        let mut outgoing = Request::builder()
            .method(parts.method.clone())
            .uri(new_uri)
            .version(parts.version)
            .body(Full::new(body_bytes.clone()))
            .expect("request reconstruida siempre es válida");
        *outgoing.headers_mut() = parts.headers.clone();
        if let Ok(value) = peer.ip().to_string().parse() {
            outgoing.headers_mut().insert("x-forwarded-for", value);
        }

        tracing::info!(
            "{peer} -> [{route_name}] {} {path_and_query} (intento {attempt}/{max_attempts})",
            backend.uri
        );

        // Contamos la request como "activa" mientras esperamos al backend
        // (clave para el algoritmo least-connections).
        backend.inc_active();
        let result = tokio::time::timeout(timeout, client.request(outgoing)).await;
        backend.dec_active();

        match result {
            Ok(Ok(resp)) => {
                let status = resp.status().as_u16();
                emit_request(
                    &events, id, &method, &path, &backend.uri.to_string(), &route_name,
                    status, true, attempt, start.elapsed(), &client_addr,
                );
                let (resp_parts, resp_body) = resp.into_parts();
                return Ok(Response::from_parts(resp_parts, resp_body.boxed()));
            }
            Ok(Err(err)) => {
                tracing::warn!("backend {} falló (intento {attempt}/{max_attempts}): {err}", backend.uri);
                last_status = StatusCode::BAD_GATEWAY;
                last_backend = backend.uri.to_string();
            }
            Err(_) => {
                tracing::warn!(
                    "backend {} no respondió en {}s (intento {attempt}/{max_attempts})",
                    backend.uri,
                    cfg.request_timeout_secs
                );
                last_status = StatusCode::GATEWAY_TIMEOUT;
                last_backend = backend.uri.to_string();
            }
        }
    }

    tracing::error!("ruta '{route_name}': todos los intentos fallaron para {path_and_query}");
    emit_request(
        &events, id, &method, &path, &last_backend, &route_name, last_status.as_u16(),
        false, max_attempts, start.elapsed(), &client_addr,
    );
    Ok(error_response(last_status, "backend no disponible"))
}

/// Helper para emitir el evento `Request` sin repetir tantos campos.
#[allow(clippy::too_many_arguments)]
fn emit_request(
    events: &EventTx,
    id: u64,
    method: &str,
    path: &str,
    backend: &str,
    route: &str,
    status: u16,
    ok: bool,
    attempts: u32,
    elapsed: Duration,
    client: &str,
) {
    events::emit(
        events,
        Event::Request {
            id,
            method: method.to_string(),
            path: path.to_string(),
            backend: backend.to_string(),
            route: route.to_string(),
            status,
            ok,
            attempts,
            duration_ms: elapsed.as_millis() as u64,
            client: client.to_string(),
        },
    );
}

/// Estado en JSON: todos los pools y sus backends (salud, totales, activas).
fn status_response(router: &Router) -> Response<ProxyBody> {
    let backends = snapshot_backends(router);
    let total: u64 = backends.iter().map(|b| b.requests).sum();
    let payload = serde_json::json!({
        "service": "oxide",
        "total_requests": total,
        "backends": backends,
    });
    let text = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string());
    let body = Full::new(Bytes::from(text)).map_err(|never| match never {}).boxed();
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(body)
        .expect("respuesta de estado siempre es válida")
}

/// Junta los backends de todos los pools en una lista plana etiquetada por ruta.
pub fn snapshot_backends(router: &Router) -> Vec<BackendInfo> {
    let mut out = Vec::new();
    for balancer in router.balancers() {
        for b in balancer.backends() {
            out.push(BackendInfo {
                url: b.uri.to_string(),
                healthy: b.is_healthy(),
                requests: b.request_count(),
                active: b.active(),
                route: balancer.name().to_string(),
            });
        }
    }
    out
}

/// Combina el `scheme://authority` del backend con el `path?query` del cliente.
fn build_uri(backend: &Uri, path_and_query: &str) -> anyhow::Result<Uri> {
    let scheme = backend.scheme_str().unwrap_or("http");
    let authority = backend
        .authority()
        .ok_or_else(|| anyhow::anyhow!("el backend no tiene host: {backend}"))?
        .as_str();
    let uri = format!("{scheme}://{authority}{path_and_query}");
    Ok(uri.parse()?)
}

fn error_response(status: StatusCode, msg: &str) -> Response<ProxyBody> {
    let body = Full::new(Bytes::from(msg.to_string()))
        .map_err(|never| match never {})
        .boxed();
    Response::builder()
        .status(status)
        .header("content-type", "text/plain; charset=utf-8")
        .body(body)
        .expect("respuesta de error siempre es válida")
}
