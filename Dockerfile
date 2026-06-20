# Imagen del proxy Oxide (Rust). Build multi-stage para una imagen final chica.
FROM rust:1-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
WORKDIR /app
RUN useradd -m oxide
COPY --from=build /app/target/release/oxide /usr/local/bin/oxide
# Config por defecto (sobreescribila montando tu propio config.toml).
COPY config.toml ./config.toml
USER oxide
# 8080 = proxy, 9090 = admin/dashboard
EXPOSE 8080 9090
CMD ["oxide"]
