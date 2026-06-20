//! El load balancer en sí.
//!
//! Implementa **round-robin** salteando los backends caídos: a cada request le
//! toca el siguiente backend *sano* en orden, rotando en círculo.
//!
//! El estado de salud de cada backend vive en un `AtomicBool` dentro de un
//! `Arc<Backend>`. ¿Por qué así? Porque dos partes del programa miran/escriben
//! ese estado al mismo tiempo:
//!   - el `proxy` lo *lee* para decidir si usa el backend,
//!   - el `health` checker lo *escribe* según los chequeos.
//! El `Arc` hace que ambos compartan EXACTAMENTE el mismo `Backend` (mismo
//! atomic), y el atomic permite leerlo/escribirlo sin `Mutex`.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use hyper::Uri;

/// Un backend: su URL, si está sano, y cuántas requests le tocaron.
#[derive(Debug)]
pub struct Backend {
    pub uri: Uri,
    healthy: AtomicBool,
    requests: AtomicU64,
}

impl Backend {
    /// ¿Está sano ahora mismo?
    pub fn is_healthy(&self) -> bool {
        self.healthy.load(Ordering::Relaxed)
    }

    /// Marca el backend como sano (`true`) o caído (`false`).
    pub fn set_healthy(&self, value: bool) {
        self.healthy.store(value, Ordering::Relaxed);
    }

    /// Cuántas requests se le rutearon hasta ahora (para el endpoint de estado).
    pub fn request_count(&self) -> u64 {
        self.requests.load(Ordering::Relaxed)
    }
}

/// Load balancer round-robin, seguro para usar desde múltiples tasks a la vez.
#[derive(Debug)]
pub struct Balancer {
    backends: Vec<Arc<Backend>>,
    next: AtomicUsize,
}

impl Balancer {
    /// Construye el balancer a partir de las URLs de la config.
    /// Arrancan todos como "sanos"; el health checker corrige enseguida.
    pub fn new(urls: Vec<String>) -> anyhow::Result<Self> {
        let mut backends = Vec::new();
        for url in urls {
            let uri: Uri = url
                .parse()
                .map_err(|e| anyhow::anyhow!("upstream inválido '{url}': {e}"))?;
            backends.push(Arc::new(Backend {
                uri,
                healthy: AtomicBool::new(true),
                requests: AtomicU64::new(0),
            }));
        }

        Ok(Self {
            backends,
            next: AtomicUsize::new(0),
        })
    }

    /// Elige el próximo backend **sano** de forma rotativa.
    ///
    /// Devuelve `None` si están TODOS caídos (en ese caso el proxy responde 503).
    /// Probamos hasta `len` posiciones desde el contador: así, si el siguiente
    /// en la rueda está caído, saltamos al que sigue en vez de fallar.
    pub fn next_backend(&self) -> Option<Arc<Backend>> {
        let n = self.backends.len();
        for _ in 0..n {
            let i = self.next.fetch_add(1, Ordering::Relaxed) % n;
            let backend = &self.backends[i];
            if backend.is_healthy() {
                // Contamos la request ruteada a este backend.
                backend.requests.fetch_add(1, Ordering::Relaxed);
                // Clonar el Arc es barato: solo suma 1 al contador de referencias.
                return Some(backend.clone());
            }
        }
        None
    }

    /// Da acceso a los backends (lo usa el health checker para actualizarlos).
    pub fn backends(&self) -> &[Arc<Backend>] {
        &self.backends
    }

    /// Lista de upstreams como texto (para logs).
    pub fn upstream_list(&self) -> Vec<String> {
        self.backends.iter().map(|b| b.uri.to_string()).collect()
    }
}
