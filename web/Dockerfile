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
# Bindear a todas las interfaces (si no, el server standalone de Next escucha
# solo en localhost y no es accesible desde afuera, sobre todo con host network).
ENV HOSTNAME=0.0.0.0
ENV PORT=3000
# Salida 'standalone' de Next: server.js + lo mínimo para correr.
COPY --from=build /app/.next/standalone ./
COPY --from=build /app/.next/static ./.next/static
EXPOSE 3000
CMD ["node", "server.js"]
