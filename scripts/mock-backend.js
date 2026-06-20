// Microservicio simulado para la demo de Oxide.
// Tres instancias IGUALES de esto (en puertos distintos) representan tu backend.
//
// Uso:  node mock-backend.js <puerto>
// Env:  ERROR_RATE=0.05  -> 5% de las requests responden 500 (para ver errores
//                            y reintentos en el dashboard). Por defecto 0.
const http = require("http");

const PORT = Number(process.argv[2] || 3001);
const HOST = process.env.HOST || "127.0.0.1"; // en Docker se usa 0.0.0.0
const INSTANCE = `svc-${PORT}`;
const ERROR_RATE = Number(process.env.ERROR_RATE || 0);

// Datos de juguete para que las respuestas parezcan una API real.
const USERS = [
  { id: 1, name: "Ana" },
  { id: 2, name: "Bruno" },
  { id: 3, name: "Carla" },
];
const PRODUCTS = [
  { id: "p1", name: "Teclado", price: 25000 },
  { id: "p2", name: "Monitor", price: 180000 },
];

// Cada ruta devuelve un objeto JSON. Todas incluyen "instance" para que se vea
// QUÉ microservicio atendió (y así notar el balanceo en el dashboard).
function route(method, path) {
  if (method === "GET" && path === "/") return { service: "mi-backend", instance: INSTANCE, status: "ok" };
  if (method === "GET" && path === "/health") return { status: "healthy", instance: INSTANCE };
  if (method === "GET" && path === "/api/users") return { instance: INSTANCE, users: USERS };
  if (method === "GET" && path === "/api/products") return { instance: INSTANCE, products: PRODUCTS };
  if (method === "GET" && path === "/api/orders") return { instance: INSTANCE, orders: [{ id: "o1", total: 205000 }] };
  if (method === "POST" && path === "/api/orders") return { instance: INSTANCE, created: true, id: "o" + Date.now() };
  return null;
}

const server = http.createServer((req, res) => {
  const path = req.url.split("?")[0];
  const data = route(req.method, path);

  // Latencia artificial (15-135ms) para que se vean tiempos realistas.
  const latency = 15 + Math.floor(Math.random() * 120);

  setTimeout(() => {
    res.setHeader("content-type", "application/json");
    res.setHeader("x-served-by", INSTANCE);

    // El /health nunca falla, para no sacarse a sí mismo de rotación.
    if (path !== "/health" && Math.random() < ERROR_RATE) {
      res.writeHead(500);
      res.end(JSON.stringify({ instance: INSTANCE, error: "fallo simulado" }));
      return;
    }

    if (data === null) {
      res.writeHead(404);
      res.end(JSON.stringify({ instance: INSTANCE, error: "not found", path }));
      return;
    }

    res.writeHead(200);
    res.end(JSON.stringify(data));
  }, latency);
});

server.listen(PORT, HOST, () => {
  console.log(`[${INSTANCE}] microservicio escuchando en http://${HOST}:${PORT}  (error_rate=${ERROR_RATE})`);
});
