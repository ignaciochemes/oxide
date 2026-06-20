// Backend de juguete para probar Oxide.
// Uso: node toy-backend.js <puerto>
// Responde identificando su puerto, así vemos el round-robin alternando.
const http = require("http");
const port = process.argv[2] || 3001;
http
  .createServer((req, res) => {
    res.writeHead(200, { "content-type": "text/plain" });
    res.end(`Hola desde backend :${port} (path=${req.url})\n`);
  })
  .listen(port, "127.0.0.1", () => console.log(`backend escuchando en :${port}`));
