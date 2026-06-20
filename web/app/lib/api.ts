"use client";

// Cliente del panel de control: habla con la API del admin de Oxide.
const API = process.env.NEXT_PUBLIC_OXIDE_API || "http://127.0.0.1:9090";
const TOKEN = process.env.NEXT_PUBLIC_OXIDE_TOKEN; // opcional (si configuraste [admin] token)

export type ConfigResponse = {
  algorithm: string;
  backends: { url: string; weight: number }[];
  proxy_url: string;
};

export async function getConfig(): Promise<ConfigResponse> {
  const r = await fetch(`${API}/api/config`);
  if (!r.ok) throw new Error("no pude leer la configuración");
  return r.json();
}

/// Dispara unas requests de prueba A TRAVÉS del proxy, para ver el dashboard
/// reaccionar. Usa mode:"no-cors": no leemos la respuesta (no hace falta), solo
/// generamos el tráfico para que se vean los pulsos.
export async function sendTestTraffic(proxyUrl: string, n = 10) {
  const paths = ["/", "/api/users", "/api/products", "/api/orders", "/health"];
  for (let i = 0; i < n; i++) {
    fetch(`${proxyUrl}${paths[i % paths.length]}`, { mode: "no-cors" }).catch(
      () => {},
    );
    await new Promise((r) => setTimeout(r, 130));
  }
}

export function addBackend(url: string, weight: number) {
  return write("POST", "/api/backends", { url, weight });
}

export function removeBackend(url: string) {
  return write("DELETE", "/api/backends", { url });
}

export function setAlgorithm(algorithm: string) {
  return write("PUT", "/api/algorithm", { algorithm });
}

async function write(method: string, path: string, body: unknown) {
  const headers: Record<string, string> = { "content-type": "application/json" };
  if (TOKEN) headers["authorization"] = `Bearer ${TOKEN}`;

  const r = await fetch(`${API}${path}`, {
    method,
    headers,
    body: JSON.stringify(body),
  });
  const data = await r.json().catch(() => ({}));
  if (!r.ok || data.error) {
    throw new Error(data.error || `error ${r.status}`);
  }
  return data;
}
