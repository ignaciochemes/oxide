// Backend "flaky" para probar timeouts y reintentos de Oxide.
// Uso: node flaky-backend.js <puerto>
//   GET /        -> 200 (pasa el health check, queda "sano")
//   GET /fail    -> corta la conexión de golpe (simula caída -> 502 en Oxide)
//   GET /slow    -> nunca responde (simula cuelgue -> 504 por timeout en Oxide)
//   otro path    -> 200 normal
const http = require("http");
const port = process.argv[2] || 3003;
http
  .createServer((req, res) => {
    if (req.url === "/fail") {
      req.socket.destroy(); // reset abrupto de la conexión
      return;
    }
    if (req.url === "/slow") {
      return; // se queda colgado a propósito
    }
    res.writeHead(200, { "content-type": "text/plain" });
    res.end(`Hola desde FLAKY :${port} (path=${req.url})\n`);
  })
  .listen(port, "127.0.0.1", () => console.log(`flaky escuchando en :${port}`));
