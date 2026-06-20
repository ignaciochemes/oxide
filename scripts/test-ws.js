// Prueba rápida del WebSocket admin de Oxide (Node 21+ trae WebSocket global).
const ws = new WebSocket("ws://127.0.0.1:9090/ws");
const msgs = [];
ws.onmessage = (e) => msgs.push(e.data);
ws.onerror = (e) => {
  console.log("WS ERROR:", e.message || e);
  process.exit(1);
};
ws.onopen = async () => {
  await new Promise((r) => setTimeout(r, 300));
  for (let i = 1; i <= 4; i++) {
    try {
      await fetch("http://127.0.0.1:8080/test" + i);
    } catch {}
  }
  setTimeout(() => {
    console.log(msgs.join("\n"));
    process.exit(0);
  }, 1500);
};
