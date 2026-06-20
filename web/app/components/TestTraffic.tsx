"use client";

import { useState } from "react";
import { getConfig, sendTestTraffic } from "../lib/api";

export default function TestTraffic() {
  const [busy, setBusy] = useState(false);

  async function go() {
    setBusy(true);
    try {
      const cfg = await getConfig();
      await sendTestTraffic(cfg.proxy_url, 12);
    } catch {
      // si falla, no rompemos nada; el botón vuelve a su estado normal
    } finally {
      setBusy(false);
    }
  }

  return (
    <button className="test-btn" onClick={go} disabled={busy}>
      {busy ? "enviando…" : "▶ Probar tráfico"}
    </button>
  );
}
