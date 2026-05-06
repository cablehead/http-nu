// End-to-end browser test for examples/2048/serve.nu.
//
// Drives a real chromium instance against an isolated http-nu instance.
// Run from the repo root:
//
//   node tests-browser/2048.test.mjs
//
// Verifies the data-* wiring: a keypress fires a fetch to /move, the bus
// publishes, the SSE handler receives the impulse, and the patched board
// arrives at the page.

import { chromium } from "playwright-core";
import { spawn } from "node:child_process";

const PORT = 39200;
const BASE = `http://127.0.0.1:${PORT}`;
const srv = spawn(
  "./target/debug/http-nu",
  ["--datastar", `127.0.0.1:${PORT}`, "examples/2048/serve.nu"],
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

await page.goto(BASE);
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

async function waitForTileCount(n, timeoutMs = 3000) {
  const deadline = Date.now() + timeoutMs;
  let snap = await snapshot();
  while (snap.tiles.length !== n && Date.now() < deadline) {
    await new Promise((r) => setTimeout(r, 100));
    snap = await snapshot();
  }
  return snap;
}

const initial = await snapshot();
check("initial board has 16 background + 2 tiles", initial.children === 18, JSON.stringify(initial));
check("initial board has 2 numeric tiles", initial.tiles.length === 2);

await page.keyboard.press("l");
const afterMove = await waitForTileCount(3);
check("after one move, 3 tiles", afterMove.tiles.length === 3, JSON.stringify(afterMove));

const score = await page.evaluate(() => document.querySelector("#status")?.textContent ?? "");
check("score shows", /Score:/.test(score), score);

await page.keyboard.press("r");
const afterReset = await waitForTileCount(2);
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
