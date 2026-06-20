"use client";

import { BackendInfo, Pulse, shortName } from "../lib/useOxide";

type Props = {
  backends: BackendInfo[];
  pulses: Pulse[];
  total: number;
  connected: boolean;
  onPulseDone: (id: number) => void;
};

// Coordenadas del lienzo (viewBox). Tres columnas: clientes -> Oxide -> backends.
const W = 1000;
const H = 600;
const CLIENT = { x: 110, y: H / 2 };
const OXIDE = { x: 440, y: H / 2 };
const BACKEND_X = 830;

/** Calcula la posición Y de cada backend, repartidos verticalmente. */
function backendY(index: number, count: number): number {
  if (count <= 1) return H / 2;
  const top = 90;
  const bottom = H - 90;
  return top + (index * (bottom - top)) / (count - 1);
}

export default function InfraDiagram({
  backends,
  pulses,
  total,
  connected,
  onPulseDone,
}: Props) {
  const count = backends.length;
  const positions = backends.map((b, i) => ({
    b,
    x: BACKEND_X,
    y: backendY(i, count),
  }));
  const indexByUrl = new Map(backends.map((b, i) => [b.url, i]));

  return (
    <svg className="diagram" viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="xMidYMid meet">
      <defs>
        <radialGradient id="oxideGlow" cx="50%" cy="50%" r="50%">
          <stop offset="0%" stopColor="var(--accent)" stopOpacity="0.35" />
          <stop offset="100%" stopColor="var(--accent)" stopOpacity="0" />
        </radialGradient>
      </defs>

      {/* Aristas Oxide -> backends */}
      {positions.map(({ b, x, y }) => (
        <line
          key={`edge-${b.url}`}
          className={`edge ${b.healthy ? "edge-up" : "edge-down"}`}
          x1={OXIDE.x}
          y1={OXIDE.y}
          x2={x}
          y2={y}
        />
      ))}

      {/* Arista clientes -> Oxide */}
      <line className="edge edge-up" x1={CLIENT.x} y1={CLIENT.y} x2={OXIDE.x} y2={OXIDE.y} />

      {/* Pulsos: una request viajando de Oxide al backend elegido */}
      {pulses.map((p) => {
        const idx = indexByUrl.get(p.backend);
        if (idx === undefined) {
          // backend desconocido (ej. "sin backend"): pulso que se desvanece en Oxide
          return (
            <circle
              key={p.id}
              className="pulse pulse-err"
              cx={OXIDE.x}
              cy={OXIDE.y}
              r={6}
              onAnimationEnd={() => onPulseDone(p.id)}
            />
          );
        }
        const y = backendY(idx, count);
        return (
          <circle
            key={p.id}
            className={`pulse ${p.ok ? "pulse-ok" : "pulse-err"}`}
            cx={OXIDE.x}
            cy={OXIDE.y}
            r={6}
            style={
              {
                "--dx": `${BACKEND_X - OXIDE.x}px`,
                "--dy": `${y - OXIDE.y}px`,
              } as React.CSSProperties
            }
            onAnimationEnd={() => onPulseDone(p.id)}
          />
        );
      })}

      {/* Nodo clientes */}
      <g>
        <circle className="node-client" cx={CLIENT.x} cy={CLIENT.y} r={34} />
        <text className="node-label" x={CLIENT.x} y={CLIENT.y + 5} textAnchor="middle">
          clientes
        </text>
      </g>

      {/* Nodo Oxide (el load balancer) */}
      <g>
        <circle cx={OXIDE.x} cy={OXIDE.y} r={90} fill="url(#oxideGlow)" />
        <circle
          className={`node-oxide ${connected ? "online" : "offline"}`}
          cx={OXIDE.x}
          cy={OXIDE.y}
          r={52}
        />
        <text className="node-title" x={OXIDE.x} y={OXIDE.y - 4} textAnchor="middle">
          OXIDE
        </text>
        <text className="node-sub" x={OXIDE.x} y={OXIDE.y + 18} textAnchor="middle">
          {total} reqs
        </text>
      </g>

      {/* Nodos backends */}
      {positions.map(({ b, x, y }) => (
        <g key={`node-${b.url}`}>
          <rect
            className={`node-backend ${b.healthy ? "up" : "down"}`}
            x={x - 16}
            y={y - 34}
            width={150}
            height={68}
            rx={12}
          />
          <circle className={`dot ${b.healthy ? "up" : "down"}`} cx={x} cy={y - 12} r={6} />
          <text className="be-name" x={x + 18} y={y - 7}>
            {shortName(b.url)}
          </text>
          <text className="be-count" x={x + 18} y={y + 17}>
            {b.requests} reqs
          </text>
        </g>
      ))}
    </svg>
  );
}
