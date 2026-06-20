//! Carga y validación de la configuración (`config.toml`).
//!
//! `serde` + `toml` convierten el archivo de texto directamente en estos structs.
//! El atributo `#[derive(Deserialize)]` es el que hace la magia: le dice a serde
//! cómo construir el struct a partir del TOML.

use serde::Deserialize;

/// Configuración completa de Oxide, espejo de `config.toml`.
#[derive(Debug, Deserialize)]
pub struct Config {
    /// Dirección donde Oxide escucha, ej. `"127.0.0.1:8080"`.
    pub listen: String,
    /// Lista de backends entre los que se reparte la carga.
    pub upstreams: Vec<Upstream>,
    /// Configuración del health check. Es opcional en el TOML: si no está,
    /// `#[serde(default)]` usa los valores por defecto de `HealthCheck`.
    #[serde(default)]
    pub health_check: HealthCheck,
    /// Configuración del proxy (timeouts y reintentos). También opcional.
    #[serde(default)]
    pub proxy: ProxyConfig,
    /// Configuración del servidor admin / dashboard (WebSocket). Opcional.
    #[serde(default)]
    pub admin: AdminConfig,
    /// Algoritmo de balanceo. Opcional (por defecto round-robin).
    #[serde(default)]
    pub balancer: BalancerConfig,
    /// Reglas de routing por host/path. Opcional: si no hay, todo va al pool
    /// `upstreams` por defecto.
    #[serde(default)]
    pub routes: Vec<RouteConfig>,
    /// Terminación TLS (HTTPS). Opcional, desactivada por defecto.
    #[serde(default)]
    pub tls: TlsConfig,
}

/// Algoritmo con el que el balancer elige el próximo backend.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Algorithm {
    /// Rota en círculo, parejo. Ideal para backends idénticos.
    #[default]
    RoundRobin,
    /// Elige el backend con menos requests activas. Bueno si tienen latencias
    /// distintas o requests largas.
    LeastConnections,
}

/// Config del balanceo.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct BalancerConfig {
    #[serde(default)]
    pub algorithm: Algorithm,
}

/// Una regla de routing: a qué pool de backends mandar según host y/o path.
/// Se evalúan en orden; gana la primera que matchea. Si ninguna matchea, se usa
/// el pool `upstreams` por defecto.
#[derive(Debug, Deserialize, Clone)]
pub struct RouteConfig {
    /// Nombre para mostrar (en logs y dashboard). Opcional.
    #[serde(default)]
    pub name: Option<String>,
    /// Si está, la regla solo matchea cuando el header Host es exactamente este.
    #[serde(default)]
    pub host: Option<String>,
    /// Si está, la regla matchea cuando el path empieza con este prefijo.
    #[serde(default)]
    pub path_prefix: Option<String>,
    /// Backends a los que rutear cuando la regla matchea.
    pub upstreams: Vec<String>,
}

/// Terminación TLS. Lo más fácil para devs: poné `enabled = true` y, si no
/// pasás certificados, Oxide genera uno self-signed para desarrollo.
#[derive(Debug, Deserialize, Clone)]
pub struct TlsConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Ruta al certificado en PEM. Si falta, se genera uno self-signed.
    #[serde(default)]
    pub cert_path: Option<String>,
    /// Ruta a la clave privada en PEM (junto con `cert_path`).
    #[serde(default)]
    pub key_path: Option<String>,
    /// Forzar certificado self-signed aunque haya paths (útil en dev).
    #[serde(default)]
    pub self_signed: bool,
    /// Hostnames para el certificado self-signed.
    #[serde(default = "default_tls_hostnames")]
    pub hostnames: Vec<String>,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cert_path: None,
            key_path: None,
            self_signed: false,
            hostnames: default_tls_hostnames(),
        }
    }
}

fn default_tls_hostnames() -> Vec<String> {
    vec!["localhost".to_string(), "127.0.0.1".to_string()]
}

/// Servidor aparte que expone el estado y el WebSocket en vivo para el dashboard.
#[derive(Debug, Deserialize, Clone)]
pub struct AdminConfig {
    /// Dirección donde escucha el admin, ej. `"127.0.0.1:9090"`.
    #[serde(default = "default_admin_listen")]
    pub listen: String,
}

impl Default for AdminConfig {
    fn default() -> Self {
        Self {
            listen: default_admin_listen(),
        }
    }
}

fn default_admin_listen() -> String {
    "127.0.0.1:9090".to_string()
}

/// Un backend (uno de tus microservicios).
#[derive(Debug, Deserialize, Clone)]
pub struct Upstream {
    /// URL base del backend, ej. `"http://127.0.0.1:3001"`.
    pub url: String,
}

/// Parámetros del chequeo de salud de los backends.
#[derive(Debug, Deserialize, Clone)]
pub struct HealthCheck {
    /// Path al que se le hace GET para chequear, ej. `"/health"`.
    #[serde(default = "default_path")]
    pub path: String,
    /// Cada cuántos segundos se chequea cada backend.
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
    /// Cuántos segundos esperar la respuesta antes de darlo por caído.
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

/// Parámetros de reenvío del proxy.
#[derive(Debug, Deserialize, Clone)]
pub struct ProxyConfig {
    /// Cuántos segundos esperar la respuesta del backend antes de cortar.
    #[serde(default = "default_request_timeout")]
    pub request_timeout_secs: u64,
    /// Cuántos reintentos extra hacer (en otro backend) si la request falla.
    /// Solo aplica a métodos idempotentes (GET, HEAD, PUT, DELETE, OPTIONS).
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Path interno que devuelve el estado de Oxide en JSON (no se proxea).
    #[serde(default = "default_status_path")]
    pub status_path: String,
}

// Valores por defecto. serde los llama cuando el campo falta en el TOML.
fn default_path() -> String {
    "/".to_string()
}
fn default_interval() -> u64 {
    5
}
fn default_timeout() -> u64 {
    2
}
fn default_request_timeout() -> u64 {
    10
}
fn default_max_retries() -> u32 {
    2
}
fn default_status_path() -> String {
    "/_oxide/status".to_string()
}

/// Permite tener un `HealthCheck` con todos los valores por defecto cuando la
/// sección `[health_check]` no aparece en el archivo.
impl Default for HealthCheck {
    fn default() -> Self {
        Self {
            path: default_path(),
            interval_secs: default_interval(),
            timeout_secs: default_timeout(),
        }
    }
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            request_timeout_secs: default_request_timeout(),
            max_retries: default_max_retries(),
            status_path: default_status_path(),
        }
    }
}

impl Config {
    /// Lee el archivo de config desde `path` y lo valida.
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&raw)?;

        // Sin upstreams no hay nada que balancear: fallamos temprano y claro.
        if config.upstreams.is_empty() {
            anyhow::bail!("la config no tiene ningún upstream definido");
        }

        Ok(config)
    }

    /// Devuelve solo las URLs de los upstreams (lo que necesita el balancer).
    pub fn upstream_urls(&self) -> Vec<String> {
        self.upstreams.iter().map(|u| u.url.clone()).collect()
    }
}
