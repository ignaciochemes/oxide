//! Routing por host/path, estilo Traefik pero mínimo.
//!
//! El `Router` tiene un pool **por defecto** y una lista de **reglas**. Para
//! cada request, recorre las reglas en orden y usa el pool de la primera que
//! matchea (por host y/o prefijo de path). Si ninguna matchea, usa el default.
//!
//! Cada pool es un `Balancer` independiente (con su propia salud y conteos),
//! así dos grupos de microservicios distintos se balancean por separado.

use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::balancer::Balancer;

/// Router compartido y reemplazable en caliente. El proxy lee siempre la versión
/// actual con `.load()`, y la recarga de config la cambia con `.store()` sin
/// frenar nada ni reiniciar el proceso.
pub type SharedRouter = Arc<ArcSwap<Router>>;

/// Condición de match de una regla. Un campo en `None` significa "no filtra por
/// eso". Con ambos en `None`, la regla matchea todo (catch-all).
#[derive(Debug, Clone)]
pub struct Matcher {
    pub host: Option<String>,
    pub path_prefix: Option<String>,
}

impl Matcher {
    fn matches(&self, host: Option<&str>, path: &str) -> bool {
        if let Some(expected) = &self.host {
            match host {
                Some(h) if h.eq_ignore_ascii_case(expected) => {}
                _ => return false,
            }
        }
        if let Some(prefix) = &self.path_prefix {
            if !path.starts_with(prefix.as_str()) {
                return false;
            }
        }
        true
    }
}

/// Una regla: una condición + el pool al que mandar si matchea.
#[derive(Debug)]
pub struct Route {
    pub matcher: Matcher,
    pub balancer: Arc<Balancer>,
}

/// El router completo.
#[derive(Debug)]
pub struct Router {
    routes: Vec<Route>,
    default: Arc<Balancer>,
}

impl Router {
    pub fn new(default: Arc<Balancer>, routes: Vec<Route>) -> Self {
        Self { routes, default }
    }

    /// Elige el pool (balancer) para un host + path dados.
    pub fn select(&self, host: Option<&str>, path: &str) -> &Arc<Balancer> {
        for route in &self.routes {
            if route.matcher.matches(host, path) {
                return &route.balancer;
            }
        }
        &self.default
    }

    /// Todos los pools (default + el de cada regla). Lo usan el health checker
    /// y el snapshot del dashboard para recorrer TODOS los backends.
    pub fn balancers(&self) -> Vec<Arc<Balancer>> {
        let mut all = Vec::with_capacity(self.routes.len() + 1);
        all.push(self.default.clone());
        all.extend(self.routes.iter().map(|r| r.balancer.clone()));
        all
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Algorithm;

    fn bal(name: &str) -> Arc<Balancer> {
        Arc::new(
            Balancer::new(
                name,
                vec![("http://127.0.0.1:1".into(), 1)],
                Algorithm::RoundRobin,
            )
            .unwrap(),
        )
    }

    #[test]
    fn matcher_combina_host_y_path() {
        let m = Matcher {
            host: Some("a.com".into()),
            path_prefix: Some("/api".into()),
        };
        assert!(m.matches(Some("a.com"), "/api/users"));
        assert!(!m.matches(Some("b.com"), "/api/users"));
        assert!(!m.matches(Some("a.com"), "/web"));
        assert!(!m.matches(None, "/api/users"));
    }

    #[test]
    fn matcher_vacio_matchea_todo() {
        let m = Matcher {
            host: None,
            path_prefix: None,
        };
        assert!(m.matches(None, "/cualquier/cosa"));
    }

    #[test]
    fn select_usa_la_primera_o_el_default() {
        let router = Router::new(
            bal("default"),
            vec![Route {
                matcher: Matcher {
                    host: None,
                    path_prefix: Some("/api".into()),
                },
                balancer: bal("api"),
            }],
        );
        assert_eq!(router.select(None, "/api/x").name(), "api");
        assert_eq!(router.select(None, "/otro").name(), "default");
    }
}
