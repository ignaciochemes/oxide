"use client";

import { useEffect, useState } from "react";
import {
  addBackend,
  ConfigResponse,
  getConfig,
  removeBackend,
  setAlgorithm,
} from "../lib/api";

const ALGOS = [
  { value: "round_robin", label: "Round-robin — reparte parejo" },
  { value: "least_connections", label: "Menos ocupado primero" },
  { value: "weighted", label: "Por peso (unos más que otros)" },
];

export default function ControlPanel() {
  const [cfg, setCfg] = useState<ConfigResponse | null>(null);
  const [url, setUrl] = useState("http://127.0.0.1:");
  const [weight, setWeight] = useState(1);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = () =>
    getConfig()
      .then(setCfg)
      .catch((e) => setError(e.message));

  useEffect(() => {
    load();
  }, []);

  async function run(fn: () => Promise<unknown>) {
    setBusy(true);
    setError(null);
    try {
      await fn();
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div>
      <div className="panel-title">Panel de control</div>
      <p className="ctrl-hint">
        Los cambios se aplican solos en ~2 segundos, sin reiniciar nada.
      </p>

      {error && <div className="ctrl-error">{error}</div>}

      <label className="ctrl-field">
        <span>¿Cómo reparto las visitas?</span>
        <select
          value={cfg?.algorithm || "round_robin"}
          disabled={busy}
          onChange={(e) => run(() => setAlgorithm(e.target.value))}
        >
          {ALGOS.map((a) => (
            <option key={a.value} value={a.value}>
              {a.label}
            </option>
          ))}
        </select>
      </label>

      <div className="ctrl-label">Servidores</div>
      <div className="ctrl-list">
        {!cfg && <div className="ctrl-muted">Cargando…</div>}
        {cfg?.backends.map((b) => (
          <div key={b.url} className="ctrl-item">
            <span className="ctrl-url">{b.url}</span>
            <span className="ctrl-weight">peso {b.weight}</span>
            <button
              className="ctrl-remove"
              disabled={busy}
              onClick={() => run(() => removeBackend(b.url))}
            >
              quitar
            </button>
          </div>
        ))}
      </div>

      <div className="ctrl-add">
        <input
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          placeholder="http://127.0.0.1:3001"
          disabled={busy}
        />
        <input
          className="ctrl-num"
          type="number"
          min={1}
          value={weight}
          onChange={(e) => setWeight(Math.max(1, Number(e.target.value)))}
          disabled={busy}
          title="peso (para 'por peso')"
        />
        <button
          className="ctrl-add-btn"
          disabled={busy || !url.startsWith("http")}
          onClick={() => run(() => addBackend(url, weight))}
        >
          + agregar servidor
        </button>
      </div>
    </div>
  );
}
