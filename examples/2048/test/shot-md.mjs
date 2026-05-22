import { chromium } from "playwright-core";
import { spawn } from "node:child_process";

const REPO_ROOT = "/root/http-nu";
const HTTP_NU = `${REPO_ROOT}/target/debug/http-nu`;
const SERVE_NU = `${REPO_ROOT}/examples/2048/serve.nu`;

const PORT = 39301;
const BASE = `http://127.0.0.1:${PORT}`;
const STORE = `/tmp/2048-shotmd-${process.pid}-${Date.now()}`;

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
const ctx = await browser.newContext({ viewport: { width: 1200, height: 1400 } });
const page = await ctx.newPage();

const targets = [
  { url: `${BASE}/design/markdown`, path: `/tmp/2048-design-markdown.png` },
  { url: `${BASE}/notes/the-rules`, path: `/tmp/2048-notes-the-rules.png` },
];

for (const t of targets) {
  await page.goto(t.url, { waitUntil: "networkidle" });
  await page.screenshot({ path: t.path, fullPage: true });
  console.log(`saved: ${t.path}`);
}

await browser.close();
cleanup();
process.exit(0);
