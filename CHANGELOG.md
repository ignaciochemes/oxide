# Changelog

Todos los cambios notables de Oxide se documentan en este archivo.

El formato está basado en [Keep a Changelog](https://keepachangelog.com/es/1.1.0/)
y el proyecto sigue [Versionado Semántico](https://semver.org/lang/es/).

## [Sin publicar]

## [0.3.0] - 2026-06-20

### Añadido
- **Algoritmo least-connections** (`[balancer] algorithm = "least_connections"`):
  elige el backend sano con menos requests en vuelo. Default sigue round-robin.
- **Routing por host/path** (estilo Traefik): `[[routes]]` con `host` y/o
  `path_prefix` mandan a su propio pool de backends; si nada matchea, va al pool
  por defecto. Cada pool se balancea y chequea por separado.
- **TLS / HTTPS** (`[tls]`): terminación TLS con rustls. Con `enabled = true` y
  sin certificados, genera uno self-signed para desarrollo; o usá `cert_path` +
  `key_path` (PEM) para producción.
- Contador de **requests activas** por backend (visible en `/status` y dashboard).
- `config.example.toml` con todas las opciones documentadas.

### Cambiado
- El proxy ahora rutea vía un `Router` (pool por defecto + reglas). El estado y
  el WebSocket exponen la ruta y las activas de cada backend.
- Graceful shutdown reimplementado por-conexión (compatible con TLS).

## [0.2.0] - 2026-06-20

### Añadido
- **Graceful shutdown**: con Ctrl+C, Oxide deja de aceptar conexiones nuevas y
  espera a que terminen las que están en curso (tope de 10s).
- **Dashboard en vivo** (`web/`, Next.js + WebSocket): diagrama ramificado de la
  infraestructura con pulsos animados por request, tarjetas de backend con salud
  y conteos, y feed de logs en tiempo real. Se reconecta solo.
- **Bus de eventos** interno (`tokio::broadcast`, `src/events.rs`): el proxy y el
  health checker publican eventos (request completada, cambio de salud).
- **Servidor admin** (`src/admin.rs`) en puerto aparte (`[admin] listen`,
  default `127.0.0.1:9090`) con `GET /status` (JSON + CORS) y WebSocket `/ws`.
- Arranque conjunto: `package.json` raíz con `bun run dev` (Oxide + dashboard
  vía `concurrently`).
- **Microservicio simulado** `scripts/mock-backend.js` (rutas tipo API, latencia
  y `ERROR_RATE` opcional) y comando `bun run demo` que levanta 3 instancias
  (3001/3002/3003) + Oxide + dashboard de una.
- Contador de requests por backend (`AtomicU64`).
- **Endpoint de estado** en `proxy.status_path` (por defecto `/_oxide/status`):
  devuelve JSON con cada backend (URL, salud, requests recibidas) y el total.
  Lo atiende Oxide mismo, no se proxea ni se cuenta como request.
- Contador de requests por backend (`AtomicU64`).
- **Timeout por request** hacia el backend (`proxy.request_timeout_secs`): si no
  responde a tiempo, se corta con `504 Gateway Timeout`.
- **Reintentos en otro backend** ante fallo o timeout (`proxy.max_retries`).
  Solo para métodos idempotentes (GET, HEAD, PUT, DELETE, OPTIONS, TRACE); los
  no idempotentes (POST, PATCH) no se reintentan para evitar efectos duplicados.
- Nueva sección opcional `[proxy]` en `config.toml`.
- Script `scripts/flaky-backend.js` y `config.test.toml` para probar
  reintentos/timeouts.

### Cambiado
- El proxy ahora bufferiza el body de la request en memoria (necesario para
  poder reenviarlo en los reintentos).

## [0.1.0] - 2026-06-20

### Añadido
- Reverse proxy HTTP a nivel L7 construido con Tokio + Hyper.
- Balanceo de carga **round-robin** entre N backends, thread-safe con
  `AtomicUsize` (sin locks).
- **Health checks** en una task de fondo: chequea cada backend periódicamente,
  lo saca de la rotación si responde mal y lo reincorpora cuando se recupera.
- Configuración por archivo `config.toml`: `listen`, `upstreams` y la sección
  opcional `[health_check]` (`path`, `interval_secs`, `timeout_secs`).
- Header `X-Forwarded-For` con la IP real del cliente hacia el backend.
- Respuestas de error: `502 Bad Gateway` si el backend elegido falla,
  `503 Service Unavailable` si están todos caídos.
- Logging estructurado con `tracing`, configurable vía `RUST_LOG`.
- Script `scripts/toy-backend.js` para levantar backends de prueba.

[Sin publicar]: https://github.com/ignaciochemes/oxide/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/ignaciochemes/oxide/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/ignaciochemes/oxide/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/ignaciochemes/oxide/releases/tag/v0.1.0
