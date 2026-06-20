"use client";

import { BackendInfo } from "../lib/useOxide";

type Props = {
  connected: boolean;
  total: number;
  backends: BackendInfo[];
};

export default function StatsBar({ connected, total, backends }: Props) {
  const healthy = backends.filter((b) => b.healthy).length;

  return (
    <div className="stats">
      <div className="stat">
        <span className="stat-label">Conexión</span>
        <span className={`stat-value ${connected ? "good" : "bad"}`}>
          <span className={`live-dot ${connected ? "on" : "off"}`} />
          {connected ? "en vivo" : "desconectado"}
        </span>
      </div>
      <div className="stat">
        <span className="stat-label">Requests totales</span>
        <span className="stat-value">{total.toLocaleString("es-AR")}</span>
      </div>
      <div className="stat">
        <span className="stat-label">Backends sanos</span>
        <span className="stat-value">
          {healthy}/{backends.length}
        </span>
      </div>
    </div>
  );
}
