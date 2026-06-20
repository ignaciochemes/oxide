"use client";

import ControlPanel from "./components/ControlPanel";
import Explainer from "./components/Explainer";
import InfraDiagram from "./components/InfraDiagram";
import LogFeed from "./components/LogFeed";
import MetricsPanel from "./components/MetricsPanel";
import StatsBar from "./components/StatsBar";
import TestTraffic from "./components/TestTraffic";
import { useOxide } from "./lib/useOxide";

export default function Page() {
  const { connected, backends, total, logs, pulses, recent, removePulse } =
    useOxide();

  return (
    <main className="page">
      <header className="header">
        <div className="brand">
          <span className="brand-mark">◆</span>
          <h1>
            Oxide <span className="brand-sub">live dashboard</span>
          </h1>
        </div>
        <div className="header-right">
          <TestTraffic />
          <StatsBar connected={connected} total={total} backends={backends} />
        </div>
      </header>

      <Explainer />

      <section className="grid">
        <div className="panel diagram-panel">
          <div className="panel-title">Infraestructura</div>
          {backends.length === 0 && (
            <div className="empty-hint">
              Todavía no hay servidores conectados. Agregá uno en el{" "}
              <b>Panel de control</b> de abajo 👇
            </div>
          )}
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

      <section className="panel metrics-panel">
        <MetricsPanel recent={recent} />
      </section>

      <section className="panel control-panel">
        <ControlPanel />
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
