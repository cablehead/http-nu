import { chromium } from "playwright-core";
import { spawn } from "node:child_process";
const PORT = 39310;
const STORE = `/tmp/2048-base-${process.pid}-${Date.now()}`;
const srv = spawn("/root/http-nu/target/debug/http-nu",
  ["--datastar", "--services", "--store", STORE, `127.0.0.1:${PORT}`, "/root/http-nu/examples/2048/serve.nu"],
  { stdio: "ignore" });
process.on("exit", () => { try { srv.kill("SIGTERM"); } catch {} });
for (let i = 0; i < 40; i++) { try { if ((await fetch(`http://127.0.0.1:${PORT}/`)).ok || true) break; } catch {} await new Promise(r => setTimeout(r, 100)); }
const b = await chromium.launch({ executablePath: "/usr/bin/chromium", args: ["--no-sandbox"] });
const ctx = await b.newContext({ viewport: { width: 1100, height: 900 } });
const p = await ctx.newPage();
await p.goto(`http://127.0.0.1:${PORT}/new`);
await p.waitForFunction(() => document.querySelectorAll(".board > div").length > 0);
for (const k of ["j","l","j","h"]) { await p.keyboard.press(k); await p.waitForTimeout(150); }
await p.waitForTimeout(300);
const tag = process.argv[2] || "x";
await p.screenshot({ path: `/tmp/2048-${tag}-play.png` });
await p.goto(`http://127.0.0.1:${PORT}/`);
await p.waitForTimeout(500);
await p.screenshot({ path: `/tmp/2048-${tag}-splash.png` });
await b.close();
process.exit(0);
