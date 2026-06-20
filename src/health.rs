//! Health checker: una task de fondo que cada N segundos le pega a cada backend
//! y actualiza su estado de salud en el `Balancer`.
//!
//! Corre en paralelo al servidor (lo lanzamos con `tokio::spawn` en `main`).
//! No bloquea nada: mientras chequea, el resto de Oxide sigue atendiendo
//! requests. Cuando un backend cambia de estado, lo logueamos.

use std::time::Duration;

use http_body_util::Empty;
use hyper::body::Bytes;
use hyper::{Request, Uri};
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;

use crate::config::HealthCheck;
use crate::events::{self, Event, EventTx};
use crate::router::SharedRouter;

/// Bucle infinito de chequeos. Se lanza una sola vez desde `main`.
/// Chequea TODOS los pools del router actual (default + cada ruta).
pub async fn run(router: SharedRouter, cfg: HealthCheck, events: EventTx) {
    // Cliente propio del health checker. Manda requests con body vacío
    // (`Empty<Bytes>`), por eso no podemos reusar el cliente del proxy, que
    // está tipado para reenviar el body entrante (`Incoming`).
    let client: Client<HttpConnector, Empty<Bytes>> =
        Client::builder(TokioExecutor::new()).build_http();

    let interval = Duration::from_secs(cfg.interval_secs);
    let timeout = Duration::from_secs(cfg.timeout_secs);

    tracing::info!(
        "health check activo: GET {} cada {}s (timeout {}s)",
        cfg.path,
        cfg.interval_secs,
        cfg.timeout_secs
    );

    loop {
        // Tomamos el router actual (puede haber cambiado por recarga) y
        // recorremos todos los pools y, dentro de cada uno, sus backends.
        let current = router.load_full();
        for balancer in current.balancers() {
            for backend in balancer.backends() {
                let healthy = check_one(&client, &backend.uri, &cfg.path, timeout).await;
                let was = backend.is_healthy();
                backend.set_healthy(healthy);

                // Solo actuamos en los cambios de estado, no en cada chequeo.
                if was != healthy {
                if healthy {
                    tracing::info!("backend {} se recupero -> UP", backend.uri);
                } else {
                    tracing::warn!("backend {} cayo -> DOWN", backend.uri);
                }
                // Avisamos al dashboard del cambio de salud.
                events::emit(
                    &events,
                    Event::BackendHealth {
                        backend: backend.uri.to_string(),
                        healthy,
                    },
                    );
                }
            }
        }

        tokio::time::sleep(interval).await;
    }
}

/// Chequea un backend: `true` si responde 2xx dentro del timeout.
async fn check_one(
    client: &Client<HttpConnector, Empty<Bytes>>,
    base: &Uri,
    path: &str,
    timeout: Duration,
) -> bool {
    let url = match health_uri(base, path) {
        Ok(uri) => uri,
        Err(_) => return false,
    };

    let req = match Request::builder().uri(url).body(Empty::<Bytes>::new()) {
        Ok(req) => req,
        Err(_) => return false,
    };

    // `tokio::time::timeout` corta la espera si el backend tarda demasiado.
    // Tres resultados posibles:
    //   Ok(Ok(resp))  -> respondió: sano solo si el status es 2xx.
    //   Ok(Err(_))    -> error de conexión (rechazó, no existe): caído.
    //   Err(_)        -> se venció el timeout: caído.
    match tokio::time::timeout(timeout, client.request(req)).await {
        Ok(Ok(resp)) => resp.status().is_success(),
        _ => false,
    }
}

/// Combina el `scheme://host:puerto` del backend con el path de salud.
fn health_uri(base: &Uri, path: &str) -> anyhow::Result<Uri> {
    let scheme = base.scheme_str().unwrap_or("http");
    let authority = base
        .authority()
        .ok_or_else(|| anyhow::anyhow!("el backend no tiene host: {base}"))?
        .as_str();

    let uri = format!("{scheme}://{authority}{path}");
    Ok(uri.parse()?)
}
