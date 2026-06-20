//! Terminación TLS (HTTPS).
//!
//! Pensado para ser fácil: con `[tls] enabled = true` y sin más, Oxide genera un
//! certificado **self-signed** para desarrollo. Si pasás `cert_path` + `key_path`
//! (PEM), usa esos (para producción, con un cert real).

use std::sync::Arc;

use anyhow::Context;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;

use crate::config::TlsConfig;

/// Construye el acceptor TLS a partir de la config.
pub fn build_acceptor(cfg: &TlsConfig) -> anyhow::Result<TlsAcceptor> {
    // Aseguramos que el proveedor de cripto 'ring' esté instalado como default.
    let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();

    let (certs, key) = if !cfg.self_signed && cfg.cert_path.is_some() {
        load_pem(cfg)?
    } else {
        generate_self_signed(&cfg.hostnames)?
    };

    let mut server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("certificado/clave TLS inválidos")?;
    // Solo HTTP/1.1 por ahora (es lo que sirve el proxy).
    server_config.alpn_protocols = vec![b"http/1.1".to_vec()];

    Ok(TlsAcceptor::from(Arc::new(server_config)))
}

/// Carga certificado + clave desde archivos PEM.
fn load_pem(cfg: &TlsConfig) -> anyhow::Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    let cert_path = cfg.cert_path.as_ref().unwrap();
    let key_path = cfg
        .key_path
        .as_ref()
        .context("falta 'tls.key_path' (la clave del certificado)")?;

    let cert_pem = std::fs::read(cert_path).with_context(|| format!("no pude leer {cert_path}"))?;
    let key_pem = std::fs::read(key_path).with_context(|| format!("no pude leer {key_path}"))?;

    let certs = rustls_pemfile::certs(&mut cert_pem.as_slice())
        .collect::<Result<Vec<_>, _>>()
        .context("no pude parsear el certificado PEM")?;
    let key = rustls_pemfile::private_key(&mut key_pem.as_slice())
        .context("no pude parsear la clave PEM")?
        .context("no encontré una clave privada en el archivo PEM")?;

    tracing::info!("TLS: usando certificado de {cert_path}");
    Ok((certs, key))
}

/// Genera un certificado self-signed en memoria (para desarrollo).
fn generate_self_signed(
    hostnames: &[String],
) -> anyhow::Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    let cert = rcgen::generate_simple_self_signed(hostnames.to_vec())
        .context("no pude generar el certificado self-signed")?;

    let cert_der = cert.cert.der().clone();
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()));

    tracing::warn!(
        "TLS: certificado SELF-SIGNED para {hostnames:?} (solo dev; el navegador mostrará 'no seguro')"
    );
    Ok((vec![cert_der], key_der))
}
