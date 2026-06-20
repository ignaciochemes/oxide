//! El load balancer en sí.
//!
//! Soporta dos algoritmos (configurables en `[balancer]`):
//!   - **round-robin**: rota en círculo entre los backends sanos. Parejo.
//!   - **least-connections**: elige el backend sano con menos requests activas.
//!
//! En ambos casos saltea los backends caídos. El estado de cada backend
//! (salud, requests totales, requests activas) vive en atomics dentro de un
//! `Arc<Backend>`, compartido entre el proxy y el health checker sin locks.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use hyper::Uri;

use crate::config::Algorithm;

/// Un backend: su URL, salud, requests totales y requests activas (en vuelo).
#[derive(Debug)]
pub struct Backend {
    pub uri: Uri,
    healthy: AtomicBool,
    requests: AtomicU64,
    active: AtomicU64,
}

impl Backend {
    pub fn is_healthy(&self) -> bool {
        self.healthy.load(Ordering::Relaxed)
    }

    pub fn set_healthy(&self, value: bool) {
        self.healthy.store(value, Ordering::Relaxed);
    }

    /// Requests totales ruteadas a este backend (acumulado).
    pub fn request_count(&self) -> u64 {
        self.requests.load(Ordering::Relaxed)
    }

    /// Requests en vuelo ahora mismo (las que todavía están siendo atendidas).
    pub fn active(&self) -> u64 {
        self.active.load(Ordering::Relaxed)
    }

    /// Marca el inicio de una request hacia este backend.
    pub fn inc_active(&self) {
        self.active.fetch_add(1, Ordering::Relaxed);
    }

    /// Marca el fin de una request hacia este backend.
    pub fn dec_active(&self) {
        self.active.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Load balancer: un pool de backends + el algoritmo para elegir entre ellos.
#[derive(Debug)]
pub struct Balancer {
    name: String,
    backends: Vec<Arc<Backend>>,
    algorithm: Algorithm,
    next: AtomicUsize,
}

impl Balancer {
    /// Construye el balancer con un nombre (para logs/dashboard), las URLs y el
    /// algoritmo. Arrancan todos "sanos"; el health checker corrige enseguida.
    pub fn new(name: impl Into<String>, urls: Vec<String>, algorithm: Algorithm) -> anyhow::Result<Self> {
        let mut backends = Vec::new();
        for url in urls {
            let uri: Uri = url
                .parse()
                .map_err(|e| anyhow::anyhow!("upstream inválido '{url}': {e}"))?;
            backends.push(Arc::new(Backend {
                uri,
                healthy: AtomicBool::new(true),
                requests: AtomicU64::new(0),
                active: AtomicU64::new(0),
            }));
        }

        Ok(Self {
            name: name.into(),
            backends,
            algorithm,
            next: AtomicUsize::new(0),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Elige el próximo backend **sano** según el algoritmo configurado.
    /// Devuelve `None` si están todos caídos.
    pub fn next_backend(&self) -> Option<Arc<Backend>> {
        // Índices de los backends sanos.
        let healthy: Vec<usize> = (0..self.backends.len())
            .filter(|&i| self.backends[i].is_healthy())
            .collect();
        if healthy.is_empty() {
            return None;
        }

        let idx = match self.algorithm {
            Algorithm::RoundRobin => {
                // Rotamos en círculo solo entre los sanos.
                let i = self.next.fetch_add(1, Ordering::Relaxed);
                healthy[i % healthy.len()]
            }
            Algorithm::LeastConnections => {
                // Mínimo de activas entre los sanos; si empatan, rotamos.
                let min = healthy
                    .iter()
                    .map(|&i| self.backends[i].active())
                    .min()
                    .unwrap();
                let candidates: Vec<usize> = healthy
                    .into_iter()
                    .filter(|&i| self.backends[i].active() == min)
                    .collect();
                let i = self.next.fetch_add(1, Ordering::Relaxed);
                candidates[i % candidates.len()]
            }
        };

        let backend = &self.backends[idx];
        backend.requests.fetch_add(1, Ordering::Relaxed);
        Some(backend.clone())
    }

    /// Acceso a los backends (lo usan el health checker y el snapshot).
    pub fn backends(&self) -> &[Arc<Backend>] {
        &self.backends
    }

    pub fn upstream_list(&self) -> Vec<String> {
        self.backends.iter().map(|b| b.uri.to_string()).collect()
    }
}
