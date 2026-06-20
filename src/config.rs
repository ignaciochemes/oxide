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
