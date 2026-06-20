"use client";

import { LogEntry } from "../lib/useOxide";

function time(ts: number): string {
  const d = new Date(ts);
  return d.toLocaleTimeString("es-AR", { hour12: false }) +
    "." + String(d.getMilliseconds()).padStart(3, "0");
}

export default function LogFeed({ logs }: { logs: LogEntry[] }) {
  return (
    <div className="logfeed">
      <div className="panel-title">Actividad en vivo</div>
      <div className="log-list">
        {logs.length === 0 && <div className="log-empty">Esperando tráfico…</div>}
        {logs.map((l) => (
          <div key={l.id} className={`log-row ${l.ok ? "ok" : "err"}`}>
            <span className="log-time">{time(l.ts)}</span>
            <span className={`log-tag ${l.kind}`}>{l.kind === "health" ? "salud" : "req"}</span>
            <span className="log-text">{l.text}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
