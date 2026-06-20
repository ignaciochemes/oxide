"use client";

import { useCallback, useEffect, useRef, useState } from "react";

// --- Tipos de los eventos que manda Oxide por WebSocket ---

export type BackendInfo = { url: string; healthy: boolean; requests: number };

export type SnapshotEvent = {
  type: "snapshot";
  backends: BackendInfo[];
  total_requests: number;
};

export type RequestEvent = {
  type: "request";
  id: number;
  method: string;
  path: string;
  backend: string;
  status: number;
  ok: boolean;
  attempts: number;
  duration_ms: number;
  client: string;
};

export type HealthEvent = {
  type: "backend_health";
  backend: string;
  healthy: boolean;
};

export type OxideEvent = SnapshotEvent | RequestEvent | HealthEvent;

// --- Estado derivado que consume la UI ---

export type LogEntry = {
  id: number;
  ts: number;
  kind: "request" | "health";
  ok: boolean;
  text: string;
};

export type Pulse = { id: number; backend: string; ok: boolean };

export type OxideState = {
  connected: boolean;
  backends: BackendInfo[];
  total: number;
  logs: LogEntry[];
  pulses: Pulse[];
  removePulse: (id: number) => void;
};

const WS_URL =
  process.env.NEXT_PUBLIC_OXIDE_WS || "ws://127.0.0.1:9090/ws";

/** Acorta una URL de backend a su host:puerto para mostrar. */
export function shortName(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

/**
 * Hook que mantiene una conexión WebSocket con Oxide y expone el estado en
 * vivo (backends, total de requests, logs y "pulsos" para animar el diagrama).
 * Se reconecta solo si la conexión se cae.
 */
export function useOxide(): OxideState {
  const [connected, setConnected] = useState(false);
  const [backends, setBackends] = useState<BackendInfo[]>([]);
  const [total, setTotal] = useState(0);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [pulses, setPulses] = useState<Pulse[]>([]);
  const seq = useRef(0);

  const removePulse = useCallback(
    (id: number) => setPulses((p) => p.filter((x) => x.id !== id)),
    [],
  );

  useEffect(() => {
    let ws: WebSocket | null = null;
    let retry: ReturnType<typeof setTimeout> | undefined;
    let closed = false;

    const connect = () => {
      ws = new WebSocket(WS_URL);

      ws.onopen = () => setConnected(true);

      ws.onclose = () => {
        setConnected(false);
        if (!closed) retry = setTimeout(connect, 2000);
      };

      ws.onerror = () => ws?.close();

      ws.onmessage = (e) => {
        let ev: OxideEvent;
        try {
          ev = JSON.parse(e.data as string);
        } catch {
          return;
        }

        if (ev.type === "snapshot") {
          setBackends(ev.backends);
          setTotal(ev.total_requests);
          return;
        }

        if (ev.type === "request") {
          setTotal((t) => t + 1);
          setBackends((bs) =>
            bs.map((b) =>
              b.url === ev.backend ? { ...b, requests: b.requests + 1 } : b,
            ),
          );
          const pid = ++seq.current;
          setPulses((p) => [...p.slice(-60), { id: pid, backend: ev.backend, ok: ev.ok }]);

          const retries = ev.attempts > 1 ? ` · ${ev.attempts} intentos` : "";
          const text = `${ev.method} ${ev.path} → ${shortName(ev.backend)} · ${ev.status} · ${ev.duration_ms}ms${retries}`;
          pushLog(setLogs, ++seq.current, "request", ev.ok, text);
          return;
        }

        if (ev.type === "backend_health") {
          setBackends((bs) =>
            bs.map((b) =>
              b.url === ev.backend ? { ...b, healthy: ev.healthy } : b,
            ),
          );
          const text = `${shortName(ev.backend)} → ${ev.healthy ? "UP" : "DOWN"}`;
          pushLog(setLogs, ++seq.current, "health", ev.healthy, text);
          return;
        }
      };
    };

    connect();

    return () => {
      closed = true;
      if (retry) clearTimeout(retry);
      ws?.close();
    };
  }, []);

  return { connected, backends, total, logs, pulses, removePulse };
}

function pushLog(
  setLogs: React.Dispatch<React.SetStateAction<LogEntry[]>>,
  id: number,
  kind: "request" | "health",
  ok: boolean,
  text: string,
) {
  setLogs((l) => [{ id, ts: Date.now(), kind, ok, text }, ...l].slice(0, 200));
}
