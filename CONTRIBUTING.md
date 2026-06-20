# Contribuir a Oxide

¡Gracias por el interés! Oxide busca ser un load balancer **simple y fácil de
usar**. Si tu cambio agrega complejidad, contá en el issue/PR por qué vale la pena.

## Setup

Necesitás [Rust](https://rustup.rs) y [Bun](https://bun.sh).

```bash
bun install        # tooling de la raíz
bun run setup      # dependencias del dashboard (web/)
bun run demo       # 3 backends de prueba + Oxide + dashboard
```

## Antes de abrir un PR

El CI corre esto; conviene pasarlo localmente:

```bash
cargo fmt --all          # formato
cargo clippy --all-targets -- -D warnings
cargo test --all         # tests
cd web && bun run build  # que el dashboard compile
```

## Estructura

- `src/` — el proxy en Rust (ver la tabla de módulos en el README).
- `web/` — el dashboard (Next.js).
- `config.example.toml` — todas las opciones documentadas.

## Ideas / roadmap

Ver la sección "Roadmap" del README. Issues etiquetados `good first issue` son un
buen punto de partida.
