//! El corazón del reverse proxy: recibe una request del cliente, elige un
//! backend con el balancer, le reenvía la request y devuelve su respuesta.
//!
//! Incluye **timeout** por intento y **reintentos** en otro backend, e **emite
//! eventos** (ver `events.rs`) para que el dashboard vea todo en vivo.
//!
//! Para poder reintentar hay que **bufferear el body** de la request: un body
//! que llega por la red (`Incoming`) es un stream que se consume una sola vez.
//! Lo juntamos en `Bytes` (que se clona barato) y reconstruimos la request en
//! cada intento. Solo reintentamos métodos **idempotentes**.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::{Method, Request, Response, StatusCode, Uri};
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;

use crate::balancer::Balancer;
use crate::config::ProxyConfig;
use crate::events::{self, Event, EventTx};

/// Tipo del cliente HTTP que usamos para hablar con los backends.
pub type ProxyClient = Client<HttpConnector, Full<Bytes>>;

/// El body que devolvemos al cliente (boxed para unificar tipos).
type ProxyBody = http_body_util::combinators::BoxBody<Bytes, hyper::Error>;

/// Contador global de requests, para darle un id único a cada una.
static REQUEST_SEQ: AtomicU64 = AtomicU64::new(1);

/// Maneja una request entrante: la reenvía a un backend sano (con reintentos),
/// emite un evento con el resultado y devuelve la respuesta.
pub async fn handle(
    req: Request<Incoming>,
    balancer: Arc<Balancer>,
    client: ProxyClient,
    cfg: ProxyConfig,
    events: EventTx,
    peer: SocketAddr,
) -> Result<Response<ProxyBody>, hyper::Error> {
    // Endpoint interno de estado: lo atiende Oxide mismo, no se proxea.
    if req.uri().path() == cfg.status_path {
        return Ok(status_response(&balancer));
    }

    let id = REQUEST_SEQ.fetch_add(1, Ordering::Relaxed);
    let start = Instant::now();
    let client_addr = peer.to_string();

    // Separamos partes + body y bufferizamos el body para poder reintentar.
    let (parts, body) = req.into_parts();
    let method = parts.method.to_string();
    let path = parts.uri.path().to_string();

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

    // ¿Idempotente? Solo esos se reintentan.
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
                tracing::error!("no hay backends sanos disponibles");
                events::emit(
                    &events,
                    Event::Request {
                        id,
                        method: method.clone(),
                        path: path.clone(),
                        backend: "(sin backend)".to_string(),
                        status: 503,
                        ok: false,
                        attempts: attempt,
                        duration_ms: start.elapsed().as_millis() as u64,
                        client: client_addr.clone(),
                    },
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
                tracing::error!("no pude armar la URI hacia {}: {err}", backend.uri);
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

        tracing::info!("{peer} -> {} {path_and_query} (intento {attempt}/{max_attempts})", backend.uri);

        match tokio::time::timeout(timeout, client.request(outgoing)).await {
            Ok(Ok(resp)) => {
                let status = resp.status().as_u16();
                events::emit(
                    &events,
                    Event::Request {
                        id,
                        method: method.clone(),
                        path: path.clone(),
                        backend: backend.uri.to_string(),
                        status,
                        ok: true,
                        attempts: attempt,
                        duration_ms: start.elapsed().as_millis() as u64,
                        client: client_addr.clone(),
                    },
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

    // Se agotaron los intentos sin éxito.
    tracing::error!("todos los intentos fallaron para {path_and_query}");
    events::emit(
        &events,
        Event::Request {
            id,
            method,
            path,
            backend: last_backend,
            status: last_status.as_u16(),
            ok: false,
            attempts: max_attempts,
            duration_ms: start.elapsed().as_millis() as u64,
            client: client_addr,
        },
    );
    Ok(error_response(last_status, "backend no disponible"))
}

/// Devuelve el estado de Oxide en JSON (mismo contenido que el snapshot del WS).
fn status_response(balancer: &Balancer) -> Response<ProxyBody> {
    let backends: Vec<_> = balancer
        .backends()
        .iter()
        .map(|b| {
            serde_json::json!({
                "url": b.uri.to_string(),
                "healthy": b.is_healthy(),
                "requests": b.request_count(),
            })
        })
        .collect();

    let total: u64 = balancer.backends().iter().map(|b| b.request_count()).sum();

    let payload = serde_json::json!({
        "service": "oxide",
        "total_requests": total,
        "backends": backends,
    });

    let text = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string());

    let body = Full::new(Bytes::from(text))
        .map_err(|never| match never {})
        .boxed();

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(body)
        .expect("respuesta de estado siempre es válida")
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

/// Arma una respuesta de error simple con un mensaje de texto en el body.
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
