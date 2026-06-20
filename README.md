# Oxide

Un **reverse proxy / load balancer** HTTP escrito en Rust, hecho para aprender y
para resolver un problema real: repartir la carga entre varios microservicios
idénticos (estilo nginx / Traefik, pero propio y mínimo).

## Estado actual (v0.1)

- Proxy HTTP a nivel **L7** (entiende HTTP).
- Balanceo **round-robin** entre N backends.
- **Health checks**: saca de la rotación los backends caídos y los reincorpora
  solos cuando se recuperan.
- Configuración por archivo `config.toml`.
- Header `X-Forwarded-For` con la IP real del cliente.
- `502 Bad Gateway` si el backend elegido falla; `503` si están todos caídos.
- Logging con `tracing` (controlable vía `RUST_LOG`).

## Cómo funciona

```
        ┌─────────┐        ┌──────────────────┐
client ─┤  Oxide  ├──┬────▶│ backend :3001    │
        └─────────┘  │     └──────────────────┘
        round-robin  │     ┌──────────────────┐
                     └────▶│ backend :3002    │
                           └──────────────────┘
```

### Estructura del código

| Archivo            | Responsabilidad                                            |
|--------------------|------------------------------------------------------------|
| `src/main.rs`      | Arranque: carga config, lanza el health checker, acepta conexiones |
| `src/config.rs`    | Lee y valida `config.toml`                                  |
| `src/balancer.rs`  | Elige el próximo backend **sano** (round-robin)            |
| `src/health.rs`    | Task de fondo que chequea la salud de cada backend         |
| `src/proxy.rs`     | Reenvía la request al backend y devuelve la respuesta      |

## Configuración

Editá `config.toml`:

```toml
listen = "127.0.0.1:8080"

[[upstreams]]
url = "http://127.0.0.1:3001"

[[upstreams]]
url = "http://127.0.0.1:3002"
```

## Uso

```bash
# Compilar y correr (modo debug)
cargo run

# Con más logs
RUST_LOG=debug cargo run        # Linux/Mac
$env:RUST_LOG="debug"; cargo run  # PowerShell

# Build optimizado para producción
cargo build --release
./target/release/oxide
```

Oxide queda escuchando en `http://127.0.0.1:8080` y reparte cada request entre
los upstreams configurados.

### Probarlo rápido

Levantá dos "backends" de juguete en otras terminales y pegale a Oxide:

```bash
# Terminal 1
python -m http.server 3001
# Terminal 2
python -m http.server 3002
# Terminal 3: cada request alterna entre 3001 y 3002
curl http://127.0.0.1:8080/
```

## Roadmap de aprendizaje

Próximos pasos pensados de menor a mayor dificultad:

1. ✅ ~~**Health checks** — sacar de la rotación los backends caídos.~~ (hecho)
2. **Algoritmos** — least-connections, weighted round-robin.
3. **Routing** — elegir backend según host o path (como Traefik).
4. **Timeouts y reintentos** — no colgarse si un backend tarda.
5. **HTTPS** — terminación TLS con `rustls`.
6. **Métricas** — endpoint `/metrics` (requests, latencias, errores).
7. **Recarga de config** — sin reiniciar el proceso.
