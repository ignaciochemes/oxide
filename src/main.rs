//! Oxide: un reverse proxy / load balancer HTTP escrito en Rust.
//!
//! Flujo general:
//!   cliente -> Oxide (este binario) -> uno de tus backends -> y de vuelta.
//!
//! `main` levanta el listener TCP y, por cada conexión entrante, lanza una task
//! de tokio que la atiende. Cada request dentro de esa conexión pasa por
//! `proxy::handle`, que elige un backend con el `Balancer` y reenvía.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::net::TcpListener;

mod balancer;
mod config;
mod health;
mod proxy;

use balancer::Balancer;
use config::Config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Logging. Controlá el nivel con la variable de entorno RUST_LOG,
    // por ejemplo: RUST_LOG=debug. Por defecto mostramos info de Oxide.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "oxide=info".into()),
        )
        .init();

    // Cargamos la config. Se puede sobreescribir el path con OXIDE_CONFIG.
    let config_path = std::env::var("OXIDE_CONFIG").unwrap_or_else(|_| "config.toml".into());
    let config = Config::load(&config_path)
        .with_context(|| format!("no pude cargar la config desde '{config_path}'"))?;

    let listen_addr: SocketAddr = config
        .listen
        .parse()
        .with_context(|| format!("dirección de 'listen' inválida: {}", config.listen))?;

    // El balancer se comparte entre TODAS las conexiones, por eso va en un Arc
    // (puntero con conteo de referencias, seguro entre threads).
    let balancer = Arc::new(Balancer::new(config.upstream_urls())?);

    // Un único cliente HTTP con pool de conexiones, también compartido.
    // `Client` es barato de clonar (internamente ya usa Arc).
    let client: proxy::ProxyClient = Client::builder(TokioExecutor::new()).build_http();

    let listener = TcpListener::bind(listen_addr)
        .await
        .with_context(|| format!("no pude escuchar en {listen_addr}"))?;

    // Lanzamos el health checker en una task de fondo. Comparte el mismo
    // `balancer` (vía Arc), así sus actualizaciones de salud las ve el proxy.
    tokio::spawn(health::run(balancer.clone(), config.health_check.clone()));

    tracing::info!("Oxide escuchando en http://{listen_addr}");
    tracing::info!("Balanceando entre: {:?}", balancer.upstream_list());

    // Bucle de aceptación: por cada conexión nueva, una task independiente.
    loop {
        let (stream, peer) = listener.accept().await?;
        let io = TokioIo::new(stream);

        // Clonamos los handles compartidos para moverlos a la task.
        let balancer = balancer.clone();
        let client = client.clone();

        tokio::spawn(async move {
            // `service_fn` adapta nuestra función `handle` a lo que espera hyper.
            let service = service_fn(move |req| {
                proxy::handle(req, balancer.clone(), client.clone(), peer)
            });

            if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
                tracing::warn!("error sirviendo la conexión de {peer}: {err:?}");
            }
        });
    }
}
