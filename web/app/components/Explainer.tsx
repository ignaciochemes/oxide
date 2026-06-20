"use client";

import { useEffect, useState } from "react";

const KEY = "oxide-explainer-dismissed";

export default function Explainer() {
  // Se muestra por defecto (para quien recién llega) y se oculta solo si ya lo
  // cerraste alguna vez.
  const [show, setShow] = useState(true);

  useEffect(() => {
    if (localStorage.getItem(KEY) === "1") setShow(false);
  }, []);

  if (!show) return null;

  const dismiss = () => {
    localStorage.setItem(KEY, "1");
    setShow(false);
  };

  return (
    <div className="explainer">
      <button className="explainer-close" onClick={dismiss} aria-label="cerrar">
        ×
      </button>
      <h2>¿Qué es esto?</h2>
      <p>
        Imaginá un negocio con varias cajas para atender. <b>Oxide</b> es quien
        recibe a cada cliente en la puerta y lo manda a la caja más libre, así
        ninguna se satura y la fila avanza rápido. Eso es un{" "}
        <b>balanceador de carga</b>.
      </p>
      <div className="explainer-flow">
        <span className="ex-node">Clientes</span>
        <span className="ex-arrow">→</span>
        <span className="ex-node ex-oxide">◆ Oxide</span>
        <span className="ex-arrow">→</span>
        <span className="ex-node">Tus servidores</span>
      </div>
      <p className="explainer-foot">
        Abajo agregás servidores y elegís cómo repartir el trabajo. Tocá{" "}
        <b>“Probar tráfico”</b> para ver las pelotitas moverse por el diagrama.
      </p>
    </div>
  );
}
