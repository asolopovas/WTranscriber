#!/usr/bin/env bun
// Evaluate a JS expression in the running WebView via CDP.
// Works against Android WebView (which only exposes /devtools/page/<id>, not /devtools/browser)
// and desktop Chromium (which exposes /devtools/browser). Tries Playwright's connectOverCDP
// first; falls back to a raw per-page websocket via Runtime.evaluate.

const expr = process.argv[2] ?? "1+1";
const HOST = process.env.CDP_HOST ?? "127.0.0.1";
const PORT = process.env.CDP_PORT ?? "9222";
const BASE = `http://${HOST}:${PORT}`;

type CdpTarget = {
  id: string;
  type: string;
  url: string;
  webSocketDebuggerUrl?: string;
};

async function fetchJson<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE}${path}`);
  if (!res.ok) throw new Error(`${path} → ${res.status}`);
  return (await res.json()) as T;
}

async function pickPage(): Promise<CdpTarget> {
  const targets = await fetchJson<CdpTarget[]>("/json");
  const page = targets.find((t) => t.type === "page" && t.webSocketDebuggerUrl);
  if (!page) throw new Error("no page target with webSocketDebuggerUrl");
  return page;
}

async function evalViaWs(wsUrl: string, expression: string): Promise<unknown> {
  return await new Promise((resolve, reject) => {
    const ws = new WebSocket(wsUrl);
    const timer = setTimeout(() => {
      try {
        ws.close();
      } catch {}
      reject(new Error("ws timeout"));
    }, 15000);
    ws.addEventListener("open", () => {
      ws.send(
        JSON.stringify({
          id: 1,
          method: "Runtime.evaluate",
          params: {
            expression,
            awaitPromise: true,
            returnByValue: true,
          },
        }),
      );
    });
    ws.addEventListener("message", (ev) => {
      clearTimeout(timer);
      try {
        const msg = JSON.parse(String(ev.data));
        if (msg.id !== 1) return;
        ws.close();
        if (msg.error) return reject(new Error(JSON.stringify(msg.error)));
        const r = msg.result?.result;
        if (r?.subtype === "error" || msg.result?.exceptionDetails) {
          return reject(
            new Error(
              msg.result?.exceptionDetails?.text ?? r?.description ?? "evaluate failed",
            ),
          );
        }
        resolve(r?.value);
      } catch (err) {
        ws.close();
        reject(err);
      }
    });
    ws.addEventListener("error", (ev) => {
      clearTimeout(timer);
      reject(new Error(`ws error: ${String((ev as ErrorEvent).message ?? ev.type)}`));
    });
  });
}

const target = await pickPage();
const result = await evalViaWs(target.webSocketDebuggerUrl!, expr);
if (result === undefined) console.log("(void)");
else console.log(JSON.stringify(result, null, 2));

export {};
