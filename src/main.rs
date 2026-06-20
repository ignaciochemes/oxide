//! Oxide: un reverse proxy / load balancer HTTP escrito en Rust.
//!
//! `main` arma el router (pool por defecto + reglas de routing) detrás de un
//! `ArcSwap` (para poder recargarlo en caliente), levanta el servidor proxy y el
//! admin/dashboard, lanza el health checker y maneja el cierre ordenado.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use arc_swap::ArcSwap;
use http_body_util::combinators::BoxBody;
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::{service_fn, Service};
use hyper::{Request, Response};
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::net::TcpListener;
use tokio::sync::watch;

mod admin;
mod balancer;
mod config;
mod configfile;
mod events;
mod health;
mod proxy;
mod router;
mod tls;

use balancer::Balancer;
use config::Config;
use router::{Matcher, Route, Router, SharedRouter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "oxide=info".into()),
        )
        .init();

    let config_path = std::env::var("OXIDE_CONFIG").unwrap_or_else(|_| "config.toml".into());
    let config = Config::load(&config_path)
        .with_context(|| format!("no pude cargar la config desde '{config_path}'"))?;

    let listen_addr: SocketAddr = config
        .listen
        .parse()
        .with_context(|| format!("dirección de 'listen' inválida: {}", config.listen))?;
    let admin_addr: SocketAddr = config.admin.listen.parse().with_context(|| {
        format!(
            "dirección de 'admin.listen' inválida: {}",
            config.admin.listen
        )
    })?;

    // Router detrás de un ArcSwap: la recarga en caliente lo reemplaza sin frenar
    // las requests en curso.
    let router: SharedRouter = Arc::new(ArcSwap::from_pointee(build_router(&config)?));

    let client: proxy::ProxyClient = Client::builder(TokioExecutor::new()).build_http();
    let proxy_cfg = config.proxy.clone();

    // TLS opcional (puede generar un certificado self-signed para dev).
    let tls_acceptor = if config.tls.enabled {
        Some(tls::build_acceptor(&config.tls).context("no pude inicializar TLS")?)
    } else {
        None
    };
    let scheme = if tls_acceptor.is_some() {
        "https"
    } else {
        "http"
    };

    let events = events::channel(512);

    // Señal de shutdown compartida (Ctrl+C -> true).
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            tracing::info!("Ctrl+C recibido: iniciando cierre ordenado...");
            let _ = shutdown_tx.send(true);
        }
    });

    // Recarga de config en caliente: vigila el archivo y reemplaza el router.
    tokio::spawn(reload_task(
        config_path.clone(),
        router.clone(),
        events.clone(),
    ));

    // Health checker (chequea todos los pools del router actual).
    tokio::spawn(health::run(
        router.clone(),
        config.health_check.clone(),
        events.clone(),
    ));

    // Servidor admin / dashboard.
    {
        let router = router.clone();
        let events = events.clone();
        let shutdown_rx = shutdown_rx.clone();
        let admin_config_path = config_path.clone();
        let token = config.admin.token.clone();
        tokio::spawn(async move {
            if let Err(err) = admin::run(
                admin_addr,
                router,
                events,
                admin_config_path,
                token,
                shutdown_rx,
            )
            .await
            {
                tracing::error!("el servidor admin falló: {err:?}");
            }
        });
    }

    // --- Servidor proxy ---
    let listener = TcpListener::bind(listen_addr)
        .await
        .with_context(|| format!("no pude escuchar en {listen_addr}"))?;

    tracing::info!("Oxide escuchando en {scheme}://{listen_addr}");
    tracing::info!("Algoritmo: {:?}", config.balancer.algorithm);
    tracing::info!("Admin/dashboard en http://{admin_addr}");

    let active = Arc::new(AtomicUsize::new(0));
    let mut shutdown_proxy = shutdown_rx.clone();

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, peer) = accepted?;

                let router = router.clone();
                let client = client.clone();
                let proxy_cfg = proxy_cfg.clone();
                let events = events.clone();
                let tls_acceptor = tls_acceptor.clone();
                let active = active.clone();
                let conn_shutdown = shutdown_rx.clone();

                tokio::spawn(async move {
                    let service = service_fn(move |req| {
                        proxy::handle(
                            req,
                            router.clone(),
                            client.clone(),
                            proxy_cfg.clone(),
                            events.clone(),
                            peer,
                        )
                    });

                    active.fetch_add(1, Ordering::Relaxed);
                    match tls_acceptor {
                        Some(acceptor) => match acceptor.accept(stream).await {
                            Ok(tls_stream) => {
                                serve_conn(TokioIo::new(tls_stream), service, conn_shutdown, peer).await;
                            }
                            Err(err) => tracing::debug!("handshake TLS con {peer} falló: {err}"),
                        },
                        None => {
                            serve_conn(TokioIo::new(stream), service, conn_shutdown, peer).await;
                        }
                    }
                    active.fetch_sub(1, Ordering::Relaxed);
                });
            }
            _ = shutdown_proxy.changed() => {
                tracing::info!("proxy: dejando de aceptar conexiones nuevas");
                break;
            }
        }
    }

    drop(listener);
    let drain = async {
        while active.load(Ordering::Relaxed) > 0 {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    tokio::select! {
        _ = drain => tracing::info!("todas las conexiones cerraron limpio"),
        _ = tokio::time::sleep(Duration::from_secs(10)) => {
            tracing::warn!("timeout de cierre (10s): forzando salida");
        }
    }

    tracing::info!("Oxide cerró ordenadamente. Chau");
    Ok(())
}

/// Construye un `Router` (pool por defecto + reglas) a partir de la config.
/// Se usa al arrancar y en cada recarga en caliente.
fn build_router(config: &Config) -> anyhow::Result<Router> {
    let algorithm = config.balancer.algorithm;

    let default_balancer = Arc::new(Balancer::new(
        "default",
        config.upstream_targets(),
        algorithm,
    )?);

    let mut routes = Vec::new();
    for (i, rc) in config.routes.iter().enumerate() {
        let name = rc
            .name
            .clone()
            .unwrap_or_else(|| format!("route-{}", i + 1));
        // Las rutas usan peso 1 por backend (el peso se configura en el pool default).
        let targets: Vec<(String, u32)> = rc.upstreams.iter().map(|u| (u.clone(), 1)).collect();
        let balancer = Arc::new(Balancer::new(name.clone(), targets, algorithm)?);
        routes.push(Route {
            matcher: Matcher {
                host: rc.host.clone(),
                path_prefix: rc.path_prefix.clone(),
            },
            balancer,
        });
        tracing::info!(
            "ruta '{name}': host={:?} path_prefix={:?} -> {:?}",
            rc.host,
            rc.path_prefix,
            rc.upstreams
        );
    }

    Ok(Router::new(default_balancer, routes))
}

/// Vigila el archivo de config (por mtime) y, ante un cambio, reconstruye el
/// router y lo reemplaza en caliente. Si la nueva config es inválida, mantiene
/// la anterior y avisa por log (no se cae). Nota: cambios de puerto/TLS sí
/// requieren reiniciar; esto recarga upstreams, rutas y algoritmo.
async fn reload_task(path: String, router: SharedRouter, events: events::EventTx) {
    let mtime = || std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    let mut last = mtime();

    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let current = mtime();
        if current == last {
            continue;
        }
        last = current;

        match Config::load(&path).and_then(|cfg| build_router(&cfg)) {
            Ok(new_router) => {
                let new_router = Arc::new(new_router);
                router.store(new_router.clone());
                tracing::info!("config recargada en caliente desde '{path}'");

                // Avisamos al dashboard del nuevo estado al instante.
                let backends = proxy::snapshot_backends(&new_router);
                let total_requests = backends.iter().map(|b| b.requests).sum();
                events::emit(
                    &events,
                    events::Event::Snapshot {
                        backends,
                        total_requests,
                    },
                );
            }
            Err(err) => {
                tracing::warn!("config inválida, mantengo la anterior: {err:#}");
            }
        }
    }
}

/// Sirve una conexión HTTP/1 y, si llega la señal de shutdown, la cierra
/// ordenadamente. Genérica sobre el IO para servir igual TCP plano o TLS.
async fn serve_conn<I, S>(
    io: TokioIo<I>,
    service: S,
    mut shutdown: watch::Receiver<bool>,
    peer: SocketAddr,
) where
    I: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    S: Service<Request<Incoming>, Response = Response<BoxBody<Bytes, hyper::Error>>>,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    S::Future: Send,
{
    let conn = http1::Builder::new().serve_connection(io, service);
    tokio::pin!(conn);

    loop {
        tokio::select! {
            result = conn.as_mut() => {
                if let Err(err) = result {
                    tracing::debug!("conexión de {peer}: {err:?}");
                }
                break;
            }
            _ = shutdown.changed() => {
                conn.as_mut().graceful_shutdown();
            }
        }
    }
}
