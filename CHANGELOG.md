# Changelog

Todos los cambios notables de Oxide se documentan en este archivo.

El formato está basado en [Keep a Changelog](https://keepachangelog.com/es/1.1.0/)
y el proyecto sigue [Versionado Semántico](https://semver.org/lang/es/).

## [Sin publicar]

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

[Sin publicar]: https://github.com/ignaciochemes/oxide/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/ignaciochemes/oxide/releases/tag/v0.1.0
