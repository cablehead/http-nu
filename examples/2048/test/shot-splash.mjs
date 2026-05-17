// Capture the splash at several viewport sizes and crop framings to
// pick the best OG image. Outputs:
//   /tmp/2048-splash-{1200x630,1600x900,1200x1200,full}.png
//
// The OG spec calls for 1200x630 (1.91:1, Facebook/Twitter cards) as
// the safe default; we also generate a square + a fullpage so the user
// can compare framings.
import { chromium } from "playwright-core";
import { spawn } from "node:child_process";

const REPO_ROOT = "/root/http-nu";
const HTTP_NU = `${REPO_ROOT}/target/debug/http-nu`;
const SERVE_NU = `${REPO_ROOT}/examples/2048/serve.nu`;

const PORT = 39401;
const BASE = `http://127.0.0.1:${PORT}`;
const STORE = `/tmp/2048-shotsplash-${process.pid}-${Date.now()}`;

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

async function shoot(label, viewport, fullPage = false) {
  const ctx = await browser.newContext({ viewport, deviceScaleFactor: 2 });
  const page = await ctx.newPage();
  await page.goto(`${BASE}/`, { waitUntil: "networkidle" });
  const path = `/tmp/2048-splash-${label}.png`;
  await page.screenshot({ path, fullPage });
  console.log(`saved: ${path}`);
  await ctx.close();
}

await shoot("1200x630", { width: 1200, height: 630 });   // OG default
await shoot("1600x900", { width: 1600, height: 900 });
await shoot("1200x1200", { width: 1200, height: 1200 }); // square (Mastodon)
await shoot("full", { width: 1200, height: 800 }, true); // full splash for context

await browser.close();
cleanup();
process.exit(0);
