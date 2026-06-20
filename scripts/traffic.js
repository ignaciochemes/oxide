// Genera tráfico continuo contra Oxide para ver el dashboard en movimiento.
// Uso: node traffic.js
const reqs = [
  ["GET", "/"],
  ["GET", "/api/users"],
  ["GET", "/api/products"],
  ["GET", "/api/orders"],
  ["GET", "/health"],
  ["POST", "/api/orders"],
];
let i = 0;
setInterval(async () => {
  const [method, path] = reqs[i++ % reqs.length];
  try {
    await fetch("http://127.0.0.1:8080" + path, { method });
  } catch {}
}, 200);
