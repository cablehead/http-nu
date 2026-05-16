// End-to-end browser test for examples/2048/serve.nu.
//
// Drives a real chromium instance against an isolated http-nu instance.
// Run via check.sh, or directly:
//
//   NODE_PATH=examples/2048/node_modules node examples/2048/test.mjs
//
// Verifies the data-* wiring: a keypress fires a fetch to /move, the bus
// publishes, the SSE handler receives the impulse, and the patched board
// arrives at the page.

import { chromium } from "playwright-core";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

// Resolve repo root and serve.nu path relative to this file so the test
// works regardless of the caller's CWD.
const HERE = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(HERE, "..", "..");
const HTTP_NU = resolve(REPO_ROOT, "target", "debug", "http-nu");
const SERVE_NU = resolve(HERE, "serve.nu");

const PORT = 39200;
const BASE = `http://127.0.0.1:${PORT}`;
// Each test run gets a fresh ephemeral xs store so the event log starts empty.
const STORE = `/tmp/2048-test-${process.pid}-${Date.now()}`;
const srv = spawn(
  HTTP_NU,
  ["--datastar", "--services", "--store", STORE, `127.0.0.1:${PORT}`, SERVE_NU],
  { stdio: "ignore" },
);
const cleanup = () => { try { srv.kill("SIGTERM"); } catch {} };
process.on("exit", cleanup);
process.on("SIGINT", () => { cleanup(); process.exit(130); });

async function waitReady() {
  for (let i = 0; i < 40; i++) {
    try { if ((await fetch(`${BASE}/`)).ok) return; } catch {}
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error("server didn't come up");
}
await waitReady();

const failures = [];
function check(label, ok, detail) {
  console.log(`  ${ok ? "ok  " : "FAIL"} ${label}${ok || !detail ? "" : ` -- ${detail}`}`);
  if (!ok) failures.push(label);
}

const browser = await chromium.launch({
  executablePath: "/usr/bin/chromium",
  args: ["--no-sandbox", "--disable-dev-shm-usage"],
});
const ctx = await browser.newContext();
const page = await ctx.newPage();
page.on("pageerror", (err) => console.log(`  [pageerror] ${err.message}`));

// /new mints a games_topic frame and 302s to /play/<game-id>.
await page.goto(`${BASE}/new`);
await page.waitForFunction(
  () => document.querySelectorAll("#board > div").length > 0,
  null,
  { timeout: 5000 },
);

function snapshot() {
  return page.evaluate(() => {
    const board = document.querySelector("#board");
    return {
      children: board?.children.length ?? 0,
      tiles: Array.from(board?.children ?? [])
        .filter((c) => c.textContent && c.textContent.trim() !== "")
        .map((c) => c.textContent.trim()),
    };
  });
}

async function waitFor(predicate, timeoutMs = 3000) {
  const deadline = Date.now() + timeoutMs;
  let snap = await snapshot();
  while (!predicate(snap) && Date.now() < deadline) {
    await new Promise((r) => setTimeout(r, 100));
    snap = await snapshot();
  }
  return snap;
}

// Wait for the SSE init patch to replace the server-rendered empty placeholder.
const initial = await waitFor((s) => s.tiles.length === 2);
check("initial board has 16 background + 2 tiles", initial.children === 18, JSON.stringify(initial));
check("initial board has 2 numeric tiles", initial.tiles.length === 2);

const initialKey = JSON.stringify(initial.tiles.sort());
// Two random initial tiles can land in a configuration where some
// direction is a no-op (both already at that edge, no merge possible).
// Pressing each of the four directions guarantees at least one will
// move or merge.
for (const k of ["l", "j", "h", "k"]) {
  await page.keyboard.press(k);
  await page.waitForTimeout(150);
}
const afterMoves = await snapshot();
check(
  "board changed after pressing all four directions",
  JSON.stringify(afterMoves.tiles.sort()) !== initialKey,
  JSON.stringify(afterMoves),
);

const score = await page.evaluate(() => document.querySelector(".track-bar-top")?.textContent ?? "");
check("score shows", /Score\s+\d/.test(score), score);

// Reset is now "navigate to /new" -- mints a fresh game and redirects.
await page.goto(`${BASE}/new`);
await page.waitForFunction(
  () => document.querySelectorAll("#board > div").length > 0,
  null,
  { timeout: 5000 },
);
const afterReset = await waitFor((s) => s.tiles.length === 2);
check("reset back to 2 tiles", afterReset.tiles.length === 2, JSON.stringify(afterReset));

await browser.close();
cleanup();

if (failures.length) {
  console.log(`\n${failures.length} failure(s)`);
  process.exit(1);
} else {
  console.log("\nall ok");
  process.exit(0);
}
