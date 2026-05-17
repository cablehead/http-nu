// Snapshot tile transforms over time after a move to see if VT animates.
import { chromium } from "playwright-core";
import { spawn } from "node:child_process";

const REPO = "/root/http-nu";
const PORT = 39602;
const BASE = `http://127.0.0.1:${PORT}`;
const STORE = `/tmp/2048-pv2-${process.pid}-${Date.now()}`;

const srv = spawn(
  `${REPO}/target/debug/http-nu`,
  ["--datastar", "--services", "--store", STORE, `127.0.0.1:${PORT}`, `${REPO}/examples/2048/serve.nu`],
  { stdio: "ignore" },
);
process.on("exit", () => { try { srv.kill("SIGTERM"); } catch {} });

for (let i = 0; i < 40; i++) {
  try { if ((await fetch(`${BASE}/`)).ok) break; } catch {}
  await new Promise((r) => setTimeout(r, 100));
}

const browser = await chromium.launch({ executablePath: "/usr/bin/chromium", args: ["--no-sandbox"] });
const page = await (await browser.newContext({ viewport: { width: 800, height: 800 } })).newPage();
await page.goto(`${BASE}/new`);
await page.waitForFunction(() => document.querySelectorAll(".board > div:not(:empty)").length >= 2, { timeout: 5000 });

// Probe: while the VT runs, the ::view-transition-group pseudos animate.
// We can't query pseudos directly but we can check getAnimations() on doc.
await page.evaluate(() => {
  window.__animSnaps = [];
  const orig = document.startViewTransition.bind(document);
  document.startViewTransition = (cb) => {
    const t = orig(cb);
    t.ready.then(() => {
      const sample = () => {
        const animations = document.getAnimations();
        const vtAnims = animations.filter(a => a.effect?.pseudoElement?.startsWith("::view-transition") || (a.effect?.target?.tagName === undefined && a.constructor.name.includes("Animation")));
        window.__animSnaps.push({
          when: performance.now(),
          totalAnimations: animations.length,
          vtCount: vtAnims.length,
          sample: vtAnims.slice(0,3).map(a => ({
            pseudo: a.effect?.pseudoElement || a.effect?.target?.tagName,
            duration: a.effect?.getTiming?.()?.duration,
            playState: a.playState,
            currentTime: a.currentTime,
          })),
        });
      };
      sample();
      setTimeout(sample, 30);
      setTimeout(sample, 80);
      setTimeout(sample, 150);
      setTimeout(sample, 250);
    });
    return t;
  };
});

await page.keyboard.press("j");
await page.waitForTimeout(700);

const snaps = await page.evaluate(() => window.__animSnaps);
console.log(JSON.stringify(snaps, null, 2));

await browser.close();
process.exit(0);
