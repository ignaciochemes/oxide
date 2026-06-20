//! Oxide: un reverse proxy / load balancer HTTP escrito en Rust.
//!
//! Flujo general:
//!   cliente -> Oxide (este binario) -> uno de tus backends -> y de vuelta.
//!
//! `main` levanta DOS servidores:
//!   - el **proxy** (puerto del `listen`): atiende el tráfico real.
//!   - el **admin/dashboard** (puerto `admin.listen`): expone el estado y un
//!     WebSocket con los eventos en vivo.
//! Y maneja **graceful shutdown**: con Ctrl+C deja de aceptar conexiones nuevas
//! y espera (con un límite) a que terminen las que están en curso.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::graceful::GracefulShutdown;
use tokio::net::TcpListener;
use tokio::sync::watch;

mod admin;
mod balancer;
mod config;
mod events;
mod health;
mod proxy;

use balancer::Balancer;
use config::Config;

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
    let admin_addr: SocketAddr = config
        .admin
        .listen
        .parse()
        .with_context(|| format!("dirección de 'admin.listen' inválida: {}", config.admin.listen))?;

    // Estado compartido.
    let balancer = Arc::new(Balancer::new(config.upstream_urls())?);
    let client: proxy::ProxyClient = Client::builder(TokioExecutor::new()).build_http();
    let proxy_cfg = config.proxy.clone();

    // Bus de eventos para el dashboard en vivo.
    let events = events::channel(512);

    // Señal de shutdown: un canal `watch` que pasa de `false` a `true` cuando
    // llega Ctrl+C. Todos los servidores la escuchan.
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            tracing::info!("Ctrl+C recibido: iniciando cierre ordenado...");
            let _ = shutdown_tx.send(true);
        }
    });

    // Health checker en background.
    tokio::spawn(health::run(
        balancer.clone(),
        config.health_check.clone(),
        events.clone(),
    ));

    // Servidor admin / dashboard en background.
    {
        let balancer = balancer.clone();
        let events = events.clone();
        let shutdown_rx = shutdown_rx.clone();
        tokio::spawn(async move {
            if let Err(err) = admin::run(admin_addr, balancer, events, shutdown_rx).await {
                tracing::error!("el servidor admin falló: {err:?}");
            }
        });
    }

    // --- Servidor proxy (en este hilo principal) ---
    let listener = TcpListener::bind(listen_addr)
        .await
        .with_context(|| format!("no pude escuchar en {listen_addr}"))?;

    tracing::info!("Oxide escuchando en http://{listen_addr}");
    tracing::info!("Balanceando entre: {:?}", balancer.upstream_list());

    // `GracefulShutdown` lleva la cuenta de las conexiones vivas para poder
    // esperarlas al cerrar.
    let graceful = GracefulShutdown::new();
    let mut shutdown_proxy = shutdown_rx.clone();

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, peer) = accepted?;
                let io = TokioIo::new(stream);

                let balancer = balancer.clone();
                let client = client.clone();
                let proxy_cfg = proxy_cfg.clone();
                let events = events.clone();

                let service = service_fn(move |req| {
                    proxy::handle(
                        req,
                        balancer.clone(),
                        client.clone(),
                        proxy_cfg.clone(),
                        events.clone(),
                        peer,
                    )
                });

                let conn = http1::Builder::new().serve_connection(io, service);
                let fut = graceful.watch(conn);
                tokio::spawn(async move {
                    if let Err(err) = fut.await {
                        tracing::warn!("error sirviendo la conexión de {peer}: {err:?}");
                    }
                });
            }
            _ = shutdown_proxy.changed() => {
                tracing::info!("proxy: dejando de aceptar conexiones nuevas");
                break;
            }
        }
    }

    // Dejamos de escuchar y esperamos a las conexiones en curso (con tope).
    drop(listener);
    tokio::select! {
        _ = graceful.shutdown() => tracing::info!("todas las conexiones cerraron limpio"),
        _ = tokio::time::sleep(Duration::from_secs(10)) => {
            tracing::warn!("timeout de cierre (10s): forzando salida");
        }
    }

    tracing::info!("Oxide cerró ordenadamente. Chau 👋");
    Ok(())
}
