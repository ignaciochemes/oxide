"use client";

import InfraDiagram from "./components/InfraDiagram";
import LogFeed from "./components/LogFeed";
import StatsBar from "./components/StatsBar";
import { useOxide } from "./lib/useOxide";

export default function Page() {
  const { connected, backends, total, logs, pulses, removePulse } = useOxide();

  return (
    <main className="page">
      <header className="header">
        <div className="brand">
          <span className="brand-mark">◆</span>
          <h1>
            Oxide <span className="brand-sub">live dashboard</span>
          </h1>
        </div>
        <StatsBar connected={connected} total={total} backends={backends} />
      </header>

      <section className="grid">
        <div className="panel diagram-panel">
          <div className="panel-title">Infraestructura</div>
          <InfraDiagram
            backends={backends}
            pulses={pulses}
            total={total}
            connected={connected}
            onPulseDone={removePulse}
          />
        </div>

        <div className="panel feed-panel">
          <LogFeed logs={logs} />
        </div>
      </section>

      {!connected && (
        <div className="banner">
          No hay conexión con Oxide. ¿Está corriendo? (admin en{" "}
          <code>ws://127.0.0.1:9090/ws</code>)
        </div>
      )}
    </main>
  );
}
