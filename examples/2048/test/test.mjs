// End-to-end browser smoke for examples/2048/serve.nu.
//
// Drives a real chromium instance against an isolated http-nu. Today
// it covers what's checkable from outside the WC shadow root: splash
// CTA, /new -> /play/<id> routing, no JS errors on page load, and the
// `<game-board>` host element mounting. The pre-WC version of this
// test probed `.board > div` and similar selectors that no longer
// reach the in-shadow board; those board-level assertions will come
// back as direct WC tests (see TODO at the bottom).
//
// Run via check.sh, or directly:
//
//   NODE_PATH=examples/2048/node_modules node examples/2048/test.mjs

import { chromium } from "playwright-core";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(HERE, "..", "..", "..");
const HTTP_NU = resolve(REPO_ROOT, "target", "debug", "http-nu");
const SERVE_NU = resolve(HERE, "..", "serve.nu");

const PORT = 39200;
const BASE = `http://127.0.0.1:${PORT}`;
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

// / is the marketing splash: 200 with a "Play [n]ow" CTA linking to
// /new. The kbd-btn helper splits the label across spans so a literal
// "play now" substring won't match; check the CTA's href instead --
// that's the proof of life that matters.
{
  const r = await fetch(`${BASE}/`, { redirect: "manual", headers: { cookie: "" } });
  const body = await r.text();
  const hasCta = /class="kbd-btn primary"[^>]*href="\/new"/.test(body)
    || /href="\/new"[^>]*class="kbd-btn primary"/.test(body);
  check(
    "fresh visitor on / sees splash with primary CTA -> /new",
    r.status === 200 && hasCta,
    `status ${r.status}, body length ${body.length}`,
  );
}

const browser = await chromium.launch({
  executablePath: "/usr/bin/chromium",
  args: ["--no-sandbox", "--disable-dev-shm-usage"],
});
const ctx = await browser.newContext();
const page = await ctx.newPage();
const pageErrors = [];
page.on("pageerror", (err) => pageErrors.push(err.message));

// /new mints a games_topic frame and 302s to /play/<game-id>. After
// the redirect we expect a /play/<id> URL and a <game-board> host
// element to be in the DOM.
const resp = await page.goto(`${BASE}/new`);
check("/new responded 200 after redirect", resp?.status() === 200, String(resp?.status()));
check(
  "/new redirected to /play/<game-id>",
  /\/play\/[a-z0-9]+$/.test(page.url()),
  page.url(),
);

await page.waitForFunction(() => !!document.querySelector("game-board"), null, { timeout: 5000 });
check("<game-board> host mounted on /play", true);

check("no JS errors on /play load", pageErrors.length === 0, pageErrors.join(" | "));

await browser.close();
cleanup();

if (failures.length) {
  console.log(`\n${failures.length} failure(s)`);
  process.exit(1);
} else {
  console.log("\nall ok");
  process.exit(0);
}

// TODO: WC-level assertions live with the <game-board> tests (separate
// rewrite). Things to cover there:
//   - initial board renders 2 tiles after SSE init patch
//   - keypress (h/j/k/l) results in a board change
//   - undo restores prior state
//   - score updates after a scoring move
//   - body.play touch-action behavior (need shadow-piercing CSS test
//     pattern or :host(:state(...)) hook)
