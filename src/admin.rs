//! Servidor admin / dashboard.
//!
//! Corre en su propio puerto (por defecto 9090), separado del proxy, y expone:
//!   - `GET /status` -> snapshot del estado en JSON (con CORS abierto).
//!   - `GET /ws`     -> WebSocket que transmite eventos en vivo al dashboard.
//!
//! Tenerlo en un puerto aparte mantiene el listener del proxy limpio (todo lo
//! que entra ahí es tráfico real para los backends) y evita choques de rutas.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_tungstenite::tungstenite::Message;
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::watch;

use crate::balancer::Balancer;
use crate::events::{BackendInfo, Event, EventTx};

/// Levanta el servidor admin y atiende hasta que llega la señal de shutdown.
pub async fn run(
    addr: SocketAddr,
    balancer: Arc<Balancer>,
    events: EventTx,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("Admin/dashboard en http://{addr}  (WebSocket en ws://{addr}/ws)");

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, _peer) = accepted?;
                let io = TokioIo::new(stream);
                let balancer = balancer.clone();
                let events = events.clone();

                tokio::spawn(async move {
                    let service = service_fn(move |req| {
                        handle(req, balancer.clone(), events.clone())
                    });
                    // `.with_upgrades()` es indispensable para que funcione el
                    // handshake de WebSocket (que "actualiza" la conexión HTTP).
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

/// Rutea las requests del admin: WebSocket, status o 404.
async fn handle(
    mut req: Request<Incoming>,
    balancer: Arc<Balancer>,
    events: EventTx,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let path = req.uri().path().to_string();

    // Handshake de WebSocket.
    if path == "/ws" && hyper_tungstenite::is_upgrade_request(&req) {
        match hyper_tungstenite::upgrade(&mut req, None) {
            Ok((response, websocket)) => {
                let rx = events.subscribe();
                // La conexión WS se atiende en su propia task.
                tokio::spawn(serve_ws(websocket, balancer, rx));
                return Ok(response);
            }
            Err(err) => {
                return Ok(text(
                    StatusCode::BAD_REQUEST,
                    format!("error en el upgrade de WebSocket: {err}"),
                ));
            }
        }
    }

    // Snapshot en JSON (para chequeos rápidos o si el front prefiere fetch).
    if path == "/status" {
        let json = serde_json::to_string(&snapshot(&balancer)).unwrap_or_else(|_| "{}".to_string());
        let mut resp = Response::new(Full::new(Bytes::from(json)));
        let headers = resp.headers_mut();
        headers.insert("content-type", "application/json".parse().unwrap());
        headers.insert("access-control-allow-origin", "*".parse().unwrap());
        return Ok(resp);
    }

    Ok(text(StatusCode::NOT_FOUND, "not found".to_string()))
}

/// Atiende una conexión WebSocket: manda el snapshot inicial y luego reenvía
/// cada evento del bus hasta que el cliente se desconecta.
async fn serve_ws(
    websocket: hyper_tungstenite::HyperWebsocket,
    balancer: Arc<Balancer>,
    mut rx: tokio::sync::broadcast::Receiver<Event>,
) {
    let ws = match websocket.await {
        Ok(ws) => ws,
        Err(err) => {
            tracing::debug!("no se completó el WebSocket: {err}");
            return;
        }
    };

    // Separamos en parte de envío (sink) y de recepción (stream).
    let (mut sink, mut stream) = ws.split();

    // 1) Snapshot inicial: el dashboard arranca mostrando el estado actual.
    if let Ok(txt) = serde_json::to_string(&snapshot(&balancer)) {
        if sink.send(Message::Text(txt.into())).await.is_err() {
            return;
        }
    }

    // 2) A partir de acá, reenviamos cada evento que llega al bus.
    loop {
        tokio::select! {
            event = rx.recv() => match event {
                Ok(event) => {
                    if let Ok(txt) = serde_json::to_string(&event) {
                        if sink.send(Message::Text(txt.into())).await.is_err() {
                            break; // el cliente se fue
                        }
                    }
                }
                // Si el cliente es lento y se atrasa, salteamos lo perdido.
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => break,
            },
            // Leemos del cliente para detectar el cierre (y responder pings).
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(_)) => {}
                Some(Err(_)) => break,
            },
        }
    }
}

/// Arma el evento `Snapshot` con el estado actual de todos los backends.
fn snapshot(balancer: &Balancer) -> Event {
    let backends: Vec<BackendInfo> = balancer
        .backends()
        .iter()
        .map(|b| BackendInfo {
            url: b.uri.to_string(),
            healthy: b.is_healthy(),
            requests: b.request_count(),
        })
        .collect();

    let total_requests = balancer.backends().iter().map(|b| b.request_count()).sum();

    Event::Snapshot {
        backends,
        total_requests,
    }
}

/// Helper para respuestas de texto plano.
fn text(status: StatusCode, body: String) -> Response<Full<Bytes>> {
    let mut resp = Response::new(Full::new(Bytes::from(body)));
    *resp.status_mut() = status;
    resp.headers_mut()
        .insert("content-type", "text/plain; charset=utf-8".parse().unwrap());
    resp
}
