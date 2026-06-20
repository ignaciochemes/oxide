# Oxide

Un **reverse proxy / load balancer** HTTP escrito en Rust, hecho para aprender y
para resolver un problema real: repartir la carga entre varios microservicios
idénticos (estilo nginx / Traefik, pero propio y mínimo).

## Estado actual (v0.1)

- Proxy HTTP a nivel **L7** (entiende HTTP).
- Balanceo **round-robin** entre N backends.
- **Health checks**: saca de la rotación los backends caídos y los reincorpora
  solos cuando se recuperan.
- **Timeouts y reintentos**: corta requests lentas (`504`) y reintenta en otro
  backend ante fallos (solo métodos idempotentes).
- **Endpoint de estado** (`/_oxide/status`): JSON con la salud y el conteo de
  requests de cada backend.
- **Dashboard en vivo** (Next.js + WebSocket): diagrama de la infraestructura,
  pulsos animados por cada request, salud de los backends y feed de logs.
- **Graceful shutdown**: con Ctrl+C deja de aceptar conexiones y espera a las
  que están en curso (hasta 10s).
- Configuración por archivo `config.toml`.
- Header `X-Forwarded-For` con la IP real del cliente.
- `502 Bad Gateway` si el backend falla; `503` si están todos caídos; `504` si
  se agota el timeout.
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
| `src/main.rs`      | Arranque: config, health checker, admin, proxy, shutdown   |
| `src/config.rs`    | Lee y valida `config.toml`                                  |
| `src/balancer.rs`  | Elige el próximo backend **sano** (round-robin) + contadores |
| `src/health.rs`    | Task de fondo que chequea la salud de cada backend         |
| `src/proxy.rs`     | Reenvía la request al backend y devuelve la respuesta      |
| `src/events.rs`    | Bus de eventos (`broadcast`) para el dashboard             |
| `src/admin.rs`     | Servidor admin: `/status` y WebSocket `/ws`                |
| `web/`             | Dashboard en vivo (Next.js + WebSocket)                    |

## Configuración

Editá `config.toml`:

```toml
listen = "127.0.0.1:8080"

[[upstreams]]
url = "http://127.0.0.1:3001"

[[upstreams]]
url = "http://127.0.0.1:3002"

[[upstreams]]
url = "http://127.0.0.1:3003"
```

## Uso

### Demo completa (3 microservicios + Oxide + dashboard)

```bash
bun install            # una vez: instala 'concurrently' en la raíz
bun run setup          # una vez: instala dependencias del dashboard (web/)
bun run demo           # 3 backends (3001/3002/3003) + Oxide + dashboard
```

Abrí http://localhost:3000 y, en otra terminal, generá tráfico para verlo moverse:

```bash
bun run traffic        # pega a Oxide en loop
```

Para ver errores y reintentos en vivo: `ERROR_RATE=0.1 bun run mocks` (y aparte
`cargo run` + el dashboard).

### Todo junto, sin backends

```bash
bun run dev            # solo Oxide (proxy :8080 / admin :9090) + dashboard :3000
```

Después abrí el dashboard en http://localhost:3000.

### Solo el proxy

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

### Ver el estado

```bash
curl http://127.0.0.1:8080/_oxide/status
```

```json
{
  "service": "oxide",
  "total_requests": 5,
  "backends": [
    { "url": "http://127.0.0.1:3001/", "healthy": true, "requests": 3 },
    { "url": "http://127.0.0.1:3002/", "healthy": true, "requests": 2 }
  ]
}
```

## Roadmap de aprendizaje

Próximos pasos pensados de menor a mayor dificultad:

1. ✅ ~~**Health checks** — sacar de la rotación los backends caídos.~~ (hecho)
2. ✅ ~~**Timeouts y reintentos** — no colgarse si un backend tarda.~~ (hecho)
3. ✅ ~~**Dashboard en vivo** — WebSocket + Next.js.~~ (hecho)
4. ✅ ~~**Graceful shutdown** — cierre ordenado con Ctrl+C.~~ (hecho)
5. **Algoritmos** — least-connections, weighted round-robin.
6. **Routing** — elegir backend según host o path (como Traefik).
7. **HTTPS** — terminación TLS con `rustls`.
8. **Recarga de config** — sin reiniciar el proceso.
