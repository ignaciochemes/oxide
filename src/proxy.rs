//! El corazón del reverse proxy: recibe una request del cliente, elige un
//! backend con el balancer, le reenvía la request y devuelve su respuesta.

use std::net::SocketAddr;
use std::sync::Arc;

use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::{Request, Response, StatusCode, Uri};
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;

use crate::balancer::Balancer;

/// Tipo del cliente HTTP que usamos para hablar con los backends.
/// Lo creamos una sola vez en `main` y lo compartimos: mantiene un pool de
/// conexiones reutilizables hacia cada backend.
pub type ProxyClient = Client<HttpConnector, Incoming>;

/// El body que devolvemos al cliente. Es "boxed" (en el heap) porque puede
/// venir de dos lugares con tipos distintos: del backend, o un mensaje de error
/// que armamos nosotros. El box los unifica en un solo tipo.
type ProxyBody = http_body_util::combinators::BoxBody<Bytes, hyper::Error>;

/// Maneja una request entrante: la reenvía al próximo backend y devuelve la
/// respuesta. Nunca devuelve `Err`: ante un fallo del backend respondemos 502
/// (Bad Gateway), igual que hace nginx.
pub async fn handle(
    mut req: Request<Incoming>,
    balancer: Arc<Balancer>,
    client: ProxyClient,
    peer: SocketAddr,
) -> Result<Response<ProxyBody>, hyper::Error> {
    // Elegimos el próximo backend SANO. Si están todos caídos, no hay a quién
    // mandarle: respondemos 503 (Service Unavailable).
    let backend = match balancer.next_backend() {
        Some(backend) => backend,
        None => {
            tracing::error!("no hay backends sanos disponibles");
            return Ok(error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "no hay backends disponibles",
            ));
        }
    };

    // El path + query original que pidió el cliente (ej. "/api/users?id=1").
    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/")
        .to_string();

    // Reconstruimos la URI final: host/puerto del backend + path original.
    let new_uri = match build_uri(&backend.uri, &path_and_query) {
        Ok(uri) => uri,
        Err(err) => {
            tracing::error!("no pude armar la URI hacia el backend: {err}");
            return Ok(error_response(StatusCode::BAD_GATEWAY, "upstream inválido"));
        }
    };
    *req.uri_mut() = new_uri;

    // X-Forwarded-For: le decimos al backend la IP real del cliente, porque
    // desde su punto de vista la conexión viene de Oxide, no del cliente.
    if let Ok(value) = peer.ip().to_string().parse() {
        req.headers_mut().insert("x-forwarded-for", value);
    }

    tracing::info!("{peer} -> {} {}", backend.uri, path_and_query);

    // Reenviamos y esperamos la respuesta del backend.
    match client.request(req).await {
        Ok(resp) => {
            // Convertimos el body del backend a nuestro tipo "boxed".
            let (parts, body) = resp.into_parts();
            Ok(Response::from_parts(parts, body.boxed()))
        }
        Err(err) => {
            tracing::warn!("el backend {} falló: {err}", backend.uri);
            Ok(error_response(
                StatusCode::BAD_GATEWAY,
                "backend no disponible",
            ))
        }
    }
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
    // `Full` es un body de un solo trozo. Su error es `Infallible` (nunca falla),
    // así que con `map_err` lo "convertimos" al tipo de error que pide ProxyBody.
    let body = Full::new(Bytes::from(msg.to_string()))
        .map_err(|never| match never {})
        .boxed();

    Response::builder()
        .status(status)
        .header("content-type", "text/plain; charset=utf-8")
        .body(body)
        .expect("respuesta de error siempre es válida")
}
