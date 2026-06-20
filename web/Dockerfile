# Imagen del dashboard (Next.js). Context de build: la carpeta web/.
FROM oven/bun:1 AS build
WORKDIR /app
COPY package.json bun.lock ./
RUN bun install --frozen-lockfile
COPY . .
RUN bun run build

FROM node:22-slim AS run
WORKDIR /app
ENV NODE_ENV=production
# Salida 'standalone' de Next: server.js + lo mínimo para correr.
COPY --from=build /app/.next/standalone ./
COPY --from=build /app/.next/static ./.next/static
EXPOSE 3000
CMD ["node", "server.js"]
