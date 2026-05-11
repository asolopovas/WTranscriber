import { invoke } from "@tauri-apps/api/core";

type Level = "error" | "warn" | "info";

interface Payload {
  level: Level;
  message: string;
  source?: string;
  line?: number;
  column?: number;
  stack?: string;
}

let installed = false;

function send(p: Payload): void {
  invoke("log_renderer", { ...p }).catch(() => {});
}

function describe(value: unknown): { message: string; stack?: string } {
  if (value instanceof Error) {
    return { message: `${value.name}: ${value.message}`, stack: value.stack };
  }
  if (typeof value === "string") return { message: value };
  try {
    return { message: JSON.stringify(value) };
  } catch {
    return { message: String(value) };
  }
}

export function installErrorBridge(): void {
  if (installed) return;
  installed = true;

  window.addEventListener("error", (ev: ErrorEvent) => {
    const { message, stack } = describe(ev.error ?? ev.message);
    send({
      level: "error",
      message,
      source: ev.filename || undefined,
      line: ev.lineno || undefined,
      column: ev.colno || undefined,
      stack,
    });
  });

  window.addEventListener("unhandledrejection", (ev: PromiseRejectionEvent) => {
    const { message, stack } = describe(ev.reason);
    send({ level: "error", message: `unhandledrejection: ${message}`, stack });
  });

  for (const level of ["error", "warn"] as const) {
    const original = console[level].bind(console);
    console[level] = (...args: unknown[]) => {
      original(...args);
      try {
        const parts = args.map((a) => describe(a).message);
        const stacks = args.map((a) => describe(a).stack).filter((s): s is string => Boolean(s));
        send({
          level,
          message: parts.join(" "),
          stack: stacks.length ? stacks.join("\n---\n") : undefined,
        });
      } catch {}
    };
  }

  send({ level: "info", message: "renderer error bridge installed" });
}
