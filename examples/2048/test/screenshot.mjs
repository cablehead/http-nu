import { chromium } from "playwright-core";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = "/root/http-nu";
const HTTP_NU = `${REPO_ROOT}/target/debug/http-nu`;
const SERVE_NU = `${REPO_ROOT}/examples/2048/serve.nu`;

const PORT = 39201;
const BASE = `http://127.0.0.1:${PORT}`;
const STORE = `/tmp/2048-shot-${process.pid}-${Date.now()}`;

const srv = spawn(
  HTTP_NU,
  ["--datastar", "--services", "--store", STORE, `127.0.0.1:${PORT}`, SERVE_NU],
  { stdio: "ignore" },
);
const cleanup = () => { try { srv.kill("SIGTERM"); } catch {} };
process.on("exit", cleanup);

async function waitReady() {
  for (let i = 0; i < 40; i++) {
    try { if ((await fetch(`${BASE}/`)).ok) return; } catch {}
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error("server didn't come up");
}
await waitReady();

const browser = await chromium.launch({
  executablePath: "/usr/bin/chromium",
  args: ["--no-sandbox", "--disable-dev-shm-usage"],
});
const ctx = await browser.newContext({ viewport: { width: 720, height: 900 } });
const page = await ctx.newPage();

// /new mints a game and 302s to /play/<id>
await page.goto(`${BASE}/new`);
await page.waitForFunction(
  () => document.querySelectorAll(".board > div").length > 0,
  null, { timeout: 5000 },
);
// Play a few moves so the board has content
for (const k of ["j", "h", "j", "l", "j", "h"]) {
  await page.keyboard.press(k);
  await page.waitForTimeout(200);
}
await page.waitForTimeout(400);

const path = `/tmp/2048-play.png`;
await page.screenshot({ path, fullPage: true });
console.log(`saved: ${path}`);

await browser.close();
cleanup();
process.exit(0);
