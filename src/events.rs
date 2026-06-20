//! Bus de eventos en tiempo real.
//!
//! Oxide publica eventos de lo que va pasando por dentro (requests ruteadas,
//! cambios de salud de los backends) en un canal `broadcast` de tokio. El
//! servidor admin (ver `admin.rs`) se suscribe y reenvía esos eventos por
//! WebSocket al dashboard.
//!
//! `broadcast` es un canal multi-consumidor: cada suscriptor recibe una copia
//! de cada evento. Si no hay nadie suscripto, publicar es prácticamente gratis.

use serde::Serialize;
use tokio::sync::broadcast;

/// Punta de envío del bus. Se clona y se reparte a quien necesite publicar.
pub type EventTx = broadcast::Sender<Event>;

/// Crea el bus de eventos con una capacidad de buffer dada.
pub fn channel(capacity: usize) -> EventTx {
    let (tx, _rx) = broadcast::channel(capacity);
    tx
}

/// Publica un evento, ignorando el error de "no hay suscriptores".
pub fn emit(tx: &EventTx, event: Event) {
    let _ = tx.send(event);
}

/// Info de un backend para el snapshot inicial.
#[derive(Debug, Clone, Serialize)]
pub struct BackendInfo {
    pub url: String,
    pub healthy: bool,
    pub requests: u64,
    pub active: u64,
    /// Nombre del pool/ruta al que pertenece (ej. "default" o "api").
    pub route: String,
}

/// Los eventos que viajan al dashboard. `#[serde(tag = "type")]` hace que cada
/// uno se serialice como `{"type": "...", ...}`, fácil de discriminar en el front.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// Foto del estado actual, se manda apenas un cliente se conecta.
    Snapshot {
        backends: Vec<BackendInfo>,
        total_requests: u64,
    },
    /// Una request que ya terminó (con su backend, status y latencia).
    Request {
        id: u64,
        method: String,
        path: String,
        backend: String,
        /// Pool/ruta que atendió la request.
        route: String,
        status: u16,
        ok: bool,
        attempts: u32,
        duration_ms: u64,
        client: String,
    },
    /// Un backend cambió de estado de salud.
    BackendHealth { backend: String, healthy: bool },
}
