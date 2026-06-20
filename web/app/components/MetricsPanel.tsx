"use client";

import { useEffect, useState } from "react";
import { ReqSample } from "../lib/useOxide";

export default function MetricsPanel({ recent }: { recent: ReqSample[] }) {
  // Tick cada 1s para que las métricas "respiren" aunque no llegue tráfico.
  const [, setTick] = useState(0);
  useEffect(() => {
    const id = setInterval(() => setTick((n) => n + 1), 1000);
    return () => clearInterval(id);
  }, []);

  const now = Date.now();
  const win = recent.filter((s) => s.t > now - 60_000);
  const last5 = win.filter((s) => s.t > now - 5_000);

  const rps = Math.round((last5.length / 5) * 10) / 10;

  const durs = win.map((s) => s.dur).sort((a, b) => a - b);
  const avg = durs.length
    ? Math.round(durs.reduce((a, b) => a + b, 0) / durs.length)
    : 0;
  const p95 = durs.length ? durs[Math.floor(durs.length * 0.95)] ?? durs[durs.length - 1] : 0;

  const errs = win.filter((s) => !s.ok).length;
  const errRate = win.length ? Math.round((errs / win.length) * 100) : 0;

  // Desglose por familia de status.
  const fam = { ok: 0, redir: 0, client: 0, server: 0 };
  win.forEach((s) => {
    const k = Math.floor(s.status / 100);
    if (k === 2) fam.ok++;
    else if (k === 3) fam.redir++;
    else if (k === 4) fam.client++;
    else if (k >= 5) fam.server++;
  });

  // Barras: requests por segundo en los últimos 60s.
  const bars = Array.from({ length: 60 }, (_, i) => {
    const start = now - (60 - i) * 1000;
    return win.filter((s) => s.t >= start && s.t < start + 1000).length;
  });
  const max = Math.max(1, ...bars);

  return (
    <div>
      <div className="panel-title">Métricas (últimos 60s)</div>

      <div className="metric-cards">
        <Metric label="Requests / seg" value={`${rps}`} />
        <Metric label="Latencia media" value={`${avg} ms`} />
        <Metric label="Latencia p95" value={`${p95} ms`} />
        <Metric label="Errores" value={`${errRate}%`} bad={errRate > 0} />
      </div>

      <svg className="rps-chart" viewBox="0 0 600 90" preserveAspectRatio="none">
        {bars.map((v, i) => {
          const h = (v / max) * 80;
          return (
            <rect
              key={i}
              x={i * 10}
              y={88 - h}
              width={8}
              height={h}
              rx={2}
              className="rps-bar"
            />
          );
        })}
      </svg>

      <div className="status-chips">
        <Chip cls="s2xx" label="2xx ok" n={fam.ok} />
        <Chip cls="s3xx" label="3xx" n={fam.redir} />
        <Chip cls="s4xx" label="4xx" n={fam.client} />
        <Chip cls="s5xx" label="5xx error" n={fam.server} />
      </div>
    </div>
  );
}

function Metric({ label, value, bad }: { label: string; value: string; bad?: boolean }) {
  return (
    <div className="metric">
      <span className="metric-label">{label}</span>
      <span className={`metric-value ${bad ? "bad" : ""}`}>{value}</span>
    </div>
  );
}

function Chip({ cls, label, n }: { cls: string; label: string; n: number }) {
  return (
    <span className={`chip ${cls}`}>
      {label}: <b>{n}</b>
    </span>
  );
}
