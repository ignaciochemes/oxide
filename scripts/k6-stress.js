// Stress test de los backends A TRAVÉS de Oxide, con k6 (https://k6.io).
//
// Le pega al proxy (no a los backends directo), así medís todo el camino real:
// cliente -> Oxide -> backend -> respuesta. Mientras corre, abrí el dashboard
// (http://localhost:3000) para ver el balanceo y las métricas en vivo.
//
// Uso:
//   k6 run scripts/k6-stress.js
//   k6 run -e VUS=200 -e HOLD=2m scripts/k6-stress.js          # más carga
//   k6 run -e BASE_URL=http://127.0.0.1:8080 scripts/k6-stress.js
//
// Variables de entorno (todas opcionales):
//   BASE_URL  URL de Oxide            (default http://127.0.0.1:8080)
//   VUS       usuarios virtuales pico (default 50)
//   RAMP      duración de la subida   (default 30s)
//   HOLD      duración en el pico     (default 1m)

import http from "k6/http";
import { check, sleep } from "k6";
import { Rate } from "k6/metrics";

const BASE_URL = __ENV.BASE_URL || "http://127.0.0.1:8080";
const VUS = Number(__ENV.VUS || 50);

// Métrica propia: proporción de respuestas que NO fueron 2xx.
const errorRate = new Rate("errores_no_2xx");

export const options = {
  scenarios: {
    stress: {
      executor: "ramping-vus",
      startVUs: 0,
      stages: [
        { duration: __ENV.RAMP || "30s", target: VUS }, // subimos la carga
        { duration: __ENV.HOLD || "1m", target: VUS }, // la mantenemos
        { duration: "15s", target: 0 }, // bajamos
      ],
      gracefulRampDown: "10s",
    },
  },
  // Si no se cumplen, k6 termina con exit code != 0 (útil en CI).
  thresholds: {
    http_req_failed: ["rate<0.01"], // menos de 1% de requests fallidas
    http_req_duration: ["p(95)<500"], // 95% de las requests bajo 500ms
    errores_no_2xx: ["rate<0.01"],
  },
};

// Endpoints de los microservicios de ejemplo (mock-backend.js). Cambialos por
// las rutas reales de tu backend si probás contra el tuyo.
const endpoints = [
  { method: "GET", path: "/" },
  { method: "GET", path: "/api/users" },
  { method: "GET", path: "/api/products" },
  { method: "GET", path: "/api/orders" },
  { method: "POST", path: "/api/orders", body: JSON.stringify({ item: "demo" }) },
];

export default function () {
  const e = endpoints[Math.floor(Math.random() * endpoints.length)];
  const params = {
    headers: { "Content-Type": "application/json" },
    tags: { endpoint: e.path }, // así k6 desglosa métricas por endpoint
  };

  const res =
    e.method === "POST"
      ? http.post(`${BASE_URL}${e.path}`, e.body, params)
      : http.get(`${BASE_URL}${e.path}`, params);

  const ok = check(res, {
    "status 2xx": (r) => r.status >= 200 && r.status < 300,
  });
  errorRate.add(!ok);

  // Pausa corta entre requests por usuario, para simular tráfico realista.
  sleep(Math.random() * 0.5);
}
