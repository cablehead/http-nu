// Browser e2e for examples/cedar-admin/serve.nu. Mirrors examples/2048/test/test.mjs:
// spawn an isolated http-nu instance with the cedar plugin, wait for ready,
// drive chromium via playwright-core, exit non-zero on failure.
//
// Run via examples/cedar-admin/test/check.sh, or directly:
//   node examples/cedar-admin/test/test.mjs

import { chromium } from "playwright-core";
import { spawn } from "node:child_process";
import { existsSync, rmSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(HERE, "..", "..", "..");
const HTTP_NU = resolve(REPO_ROOT, "target", "debug", "http-nu");
const PLUGIN = resolve(REPO_ROOT, "target", "debug", "nu_plugin_cedar");
const SERVE_NU = resolve(HERE, "..", "serve.nu");

const PORT = 39201;
const BASE = `http://127.0.0.1:${PORT}`;
const STORE = `/tmp/cedar-admin-test-${process.pid}-${Date.now()}`;

const srv = spawn(
  HTTP_NU,
  [
    "--plugin", PLUGIN,
    "--datastar",
    "--dev",
    "--store", STORE,
    `127.0.0.1:${PORT}`,
    SERVE_NU,
  ],
  { stdio: "ignore" },
);

const cleanup = () => {
  try { srv.kill("SIGTERM"); } catch {}
  try { rmSync(STORE, { recursive: true, force: true }); } catch {}
};
process.on("exit", cleanup);
process.on("SIGINT", () => { cleanup(); process.exit(130); });

async function waitReady() {
  for (let i = 0; i < 50; i++) {
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

// --- pre-browser HTTP probes (don't need chromium for these) ---------------

{
  const r = await fetch(`${BASE}/`);
  const body = await r.text();
  check(
    "GET / -> 200 with Sign in link (anon)",
    r.status === 200 && /href="\/login"/.test(body) && /Not signed in/.test(body),
    `status ${r.status}, has-link ${/\/login/.test(body)}`,
  );
}

{
  const r = await fetch(`${BASE}/static/styles.css`);
  check(
    "GET /static/styles.css -> 200 text/css",
    r.status === 200 && (r.headers.get("content-type") || "").startsWith("text/css"),
    `status ${r.status}, ct ${r.headers.get("content-type")}`,
  );
}

{
  const r = await fetch(`${BASE}/nope`);
  check("GET /nope -> 404", r.status === 404, `got ${r.status}`);
}

{
  const r = await fetch(`${BASE}/policies/permissions`);
  const body = await r.text();
  const rowCount = (body.match(/<tr/g) || []).length;
  // 1 header row + N data rows. N grows when edit tests below run, so we
  // assert "at least 1 header + 190 data rows" (190 after our seed additions).
  check(
    "/policies/permissions renders ≥191 <tr> rows (1 header + ≥190 data)",
    r.status === 200 && rowCount >= 191,
    `status ${r.status}, rows ${rowCount}`,
  );
}

{
  const r = await fetch(`${BASE}/matrix`);
  const body = await r.text();
  check(
    "/matrix shows action sections + raw policies.cedarschema",
    r.status === 200
      && /generating 103 Cedar policies/.test(body)
      && /policies\.cedarschema/.test(body)
      && /Platform actions/.test(body)
      && /Event actions/.test(body)
      && /Team actions/.test(body)
      && /Player actions/.test(body),
    `status ${r.status}`,
  );
}

// --- /data generic CSV browser: index + per-table + 404 safety.
{
  const r = await fetch(`${BASE}/data`);
  const body = await r.text();
  const rowCount = (body.match(/<tr/g) || []).length;
  // 35 CSVs + 1 header row = 36 <tr>.
  check(
    "/data lists all 35 seed CSVs",
    r.status === 200 && rowCount === 36 && /actions/.test(body) && /users/.test(body),
    `status ${r.status}, rows ${rowCount}`,
  );
}
{
  const r = await fetch(`${BASE}/data/events`);
  const body = await r.text();
  // 4 events + 1 header = 5 <tr>.
  const rowCount = (body.match(/<tr/g) || []).length;
  check(
    "/data/events renders 4 event rows + header",
    r.status === 200 && rowCount === 5,
    `status ${r.status}, rows ${rowCount}`,
  );
}
{
  // Defence against path traversal + nonexistent.
  const r = await fetch(`${BASE}/data/no_such_table`);
  check("/data/<unknown> -> 404", r.status === 404, `got ${r.status}`);
}

// --- /check playground: exercises the full entity loader (OWNER, HEAD_COACH,
// SELF, GUARDIAN, ...). Two probes: form renders + a real POST that should
// allow (event owner editing their own event) and one that should deny
// (a coach trying to edit someone else's event).
{
  const r = await fetch(`${BASE}/check`);
  const body = await r.text();
  check(
    "/check renders the playground form",
    r.status === 200 && /Cedar check playground/.test(body) && /name="principal"/.test(body),
    `status ${r.status}`,
  );
}
{
  // OWNER allow: usr_org_001 is in events.organizer_user_id for evt_001.
  const r = await fetch(`${BASE}/check`, {
    method: "POST",
    headers: { "content-type": "application/x-www-form-urlencoded" },
    body: "principal=usr_org_001&action=EDIT_EVENT&resource_type=Event&resource_id=evt_001",
  });
  const body = await r.text();
  check(
    "/check POST: OWNER edits own event -> ALLOW",
    r.status === 200 && /badge ok/.test(body) && /ALLOW/.test(body),
    `status ${r.status}`,
  );
}
{
  // DENY: a coach is not owner/co-organizer of evt_001.
  const r = await fetch(`${BASE}/check`, {
    method: "POST",
    headers: { "content-type": "application/x-www-form-urlencoded" },
    body: "principal=usr_coach_001&action=EDIT_EVENT&resource_type=Event&resource_id=evt_001",
  });
  const body = await r.text();
  check(
    "/check POST: coach edits someone else's event -> DENY",
    r.status === 200 && /badge deny/.test(body) && /DENY/.test(body),
    `status ${r.status}`,
  );
}

// --- SSE wire-format probe -- the projection emits an initial tbody, then a
// patch per cedar.policy.edited frame. Verify both: status + content-type +
// initial event payload contains our tbody id.
{
  // Cold-spawn cedar codegen + 190-row HTML render can take 1-2s. Bump the
  // overall deadline to 5s; the inner loop aborts the moment it sees the
  // tbody marker, so the timer is just a safety net.
  const controller = new AbortController();
  const sseTimer = setTimeout(() => controller.abort(), 5000);
  let body = "";
  let status = 0;
  let contentType = "";
  try {
    const r = await fetch(`${BASE}/policies/permissions/sse`, {
      signal: controller.signal,
      headers: { accept: "text/event-stream" },
    });
    status = r.status;
    contentType = r.headers.get("content-type") || "";
    const reader = r.body.getReader();
    const decoder = new TextDecoder();
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      body += decoder.decode(value, { stream: true });
      if (body.includes("permissions-tbody")) {
        controller.abort();
        break;
      }
    }
  } catch (e) {
    // Expected: AbortError when we cut the stream
  } finally {
    clearTimeout(sseTimer);
  }
  check(
    "/policies/permissions/sse -> 200 text/event-stream with initial tbody patch",
    status === 200
      && contentType.startsWith("text/event-stream")
      && body.includes("event: datastar-patch-elements")
      && body.includes('id="permissions-tbody"'),
    `status ${status}, ct ${contentType}, body ${body.length}b`,
  );
}

// --- login flow via raw HTTP (manual cookie handling for clarity) ---------

{
  const loginRes = await fetch(`${BASE}/login`, {
    method: "POST",
    headers: { "content-type": "application/x-www-form-urlencoded" },
    body: "user_id=usr_admin_001",
    redirect: "manual",
  });
  check(
    "POST /login as usr_admin_001 -> 302 /me with session cookie",
    loginRes.status === 302
      && loginRes.headers.get("location") === "/me"
      && /session=/.test(loginRes.headers.get("set-cookie") || ""),
    `status ${loginRes.status}, cookie ${loginRes.headers.get("set-cookie")?.slice(0, 50)}`,
  );

  const cookie = loginRes.headers.get("set-cookie")?.split(";")[0] || "";

  const meRes = await fetch(`${BASE}/me`, { headers: { cookie } });
  const meBody = await meRes.text();
  check(
    "/me with admin session shows System Admin + ADMIN role",
    meRes.status === 200 && /System Admin/.test(meBody) && /ADMIN/.test(meBody),
    `status ${meRes.status}`,
  );

  const badRes = await fetch(`${BASE}/login`, {
    method: "POST",
    headers: { "content-type": "application/x-www-form-urlencoded" },
    body: "user_id=usr_does_not_exist",
    redirect: "manual",
  });
  check(
    "POST /login with bogus user_id -> 400",
    badRes.status === 400,
    `got ${badRes.status}`,
  );

  // --- edit routes: gated by EDIT_POLICY / DELETE_POLICY ---
  // Cookie from admin login above is reused.

  // Anonymous (no cookie) trying to POST a row -> 403 with forbidden body.
  const anonAdd = await fetch(`${BASE}/policies/permissions`, {
    method: "POST",
    headers: { "content-type": "application/x-www-form-urlencoded" },
    body: "action_code=BROWSE_EVENTS&relation_code=PUBLIC&event_type_code=",
    redirect: "manual",
  });
  check(
    "POST /policies/permissions as anon -> 403 (cedar deny)",
    anonAdd.status === 403,
    `got ${anonAdd.status}`,
  );

  // Non-admin signed-in user (a coach) -> 403.
  const coachLogin = await fetch(`${BASE}/login`, {
    method: "POST",
    headers: { "content-type": "application/x-www-form-urlencoded" },
    body: "user_id=usr_coach_001",
    redirect: "manual",
  });
  const coachCookie = coachLogin.headers.get("set-cookie")?.split(";")[0] || "";
  const coachAdd = await fetch(`${BASE}/policies/permissions`, {
    method: "POST",
    headers: {
      "content-type": "application/x-www-form-urlencoded",
      cookie: coachCookie,
    },
    body: "action_code=BROWSE_EVENTS&relation_code=PUBLIC&event_type_code=",
    redirect: "manual",
  });
  check(
    "POST /policies/permissions as coach -> 403 (cedar deny)",
    coachAdd.status === 403,
    `got ${coachAdd.status}`,
  );

  // Admin -> 302, row appears in next GET.
  const adminAdd = await fetch(`${BASE}/policies/permissions`, {
    method: "POST",
    headers: {
      "content-type": "application/x-www-form-urlencoded",
      cookie,
    },
    body: "action_code=TEST_ADD_BY_E2E&relation_code=PUBLIC&event_type_code=",
    redirect: "manual",
  });
  check(
    "POST /policies/permissions as admin -> 302",
    adminAdd.status === 302,
    `got ${adminAdd.status}`,
  );

  const afterAdd = await fetch(`${BASE}/policies/permissions`, { headers: { cookie } });
  const afterBody = await afterAdd.text();
  check(
    "added row visible in next GET /policies/permissions",
    afterBody.includes("TEST_ADD_BY_E2E"),
    `body length ${afterBody.length}`,
  );

  // Clean up: find the index of TEST_ADD_BY_E2E by parsing the rendered
  // table, then delete THAT specific index. Earlier "delete last row" logic
  // was fragile -- an orphan row from a prior failed run would shift the
  // index and silently delete a real seed row (we saw DELETE_POLICY get
  // clobbered this way).
  const testRowIdx = (() => {
    // Each <tr> in tbody corresponds to a row in permissions.csv, in order.
    // Find the first tbody row whose first <td> wraps the TEST code.
    const tbodyMatch = afterBody.match(/<tbody[^>]*>([\s\S]*?)<\/tbody>/);
    if (!tbodyMatch) return -1;
    const rows = tbodyMatch[1].match(/<tr[\s\S]*?<\/tr>/g) || [];
    return rows.findIndex((r) => /<code>TEST_ADD_BY_E2E<\/code>/.test(r));
  })();
  check(
    "found TEST_ADD_BY_E2E in tbody for targeted cleanup",
    testRowIdx >= 0,
    `index ${testRowIdx}`,
  );
  const adminDel = await fetch(`${BASE}/policies/permissions/delete`, {
    method: "POST",
    headers: {
      "content-type": "application/x-www-form-urlencoded",
      cookie,
    },
    body: `row=${testRowIdx}`,
    redirect: "manual",
  });
  check(
    "POST /policies/permissions/delete as admin -> 302",
    adminDel.status === 302,
    `got ${adminDel.status}`,
  );

  // After cleanup, TEST_ADD_BY_E2E should be gone.
  const afterDel = await fetch(`${BASE}/policies/permissions`, { headers: { cookie } });
  const afterDelBody = await afterDel.text();
  check(
    "deleted row gone from next GET",
    !afterDelBody.includes("TEST_ADD_BY_E2E"),
    "row still present",
  );
}

// --- browser e2e -- visual end-to-end through chromium --------------------

// Try common chromium paths; fall back to playwright-core's default if unset.
const possibleChromiumPaths = [
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  "/Applications/Chromium.app/Contents/MacOS/Chromium",
  "/usr/bin/chromium",
];
const chromiumPath = possibleChromiumPaths.find((p) => existsSync(p));

let browser;
try {
  browser = await chromium.launch({
    ...(chromiumPath ? { executablePath: chromiumPath } : {}),
    args: ["--no-sandbox", "--disable-dev-shm-usage"],
  });
} catch (e) {
  console.log(`  skip browser e2e -- chromium not available (${e.message.slice(0, 80)})`);
  console.log(failures.length ? `FAIL ${failures.length} check(s)` : "all pre-browser checks ok");
  process.exit(failures.length ? 1 : 0);
}

const ctx = await browser.newContext();
const page = await ctx.newPage();
const pageErrors = [];
page.on("pageerror", (err) => pageErrors.push(err.message));

await page.goto(`${BASE}/`);
const homeH1 = await page.locator("h1").first().textContent();
check("browser: / renders <h1>cedar-admin</h1>", homeH1 === "cedar-admin", homeH1);

await page.goto(`${BASE}/login`);
const optionCount = await page.locator("select[name=user_id] option").count();
// users.csv has 12 rows; 11 are ACTIVE (1 is PENDING_APPROVAL).
check("browser: /login dropdown has 11 active users", optionCount === 11, `got ${optionCount}`);

await page.selectOption("select[name=user_id]", "usr_admin_001");
await page.click("button[type=submit]");
await page.waitForURL(/\/me$/);
const meText = await page.locator("main").textContent();
check(
  "browser: post-login /me shows admin user details",
  meText.includes("usr_admin_001") && meText.includes("ADMIN"),
  `me text ${meText.slice(0, 80)}`,
);

// --- live-update via Datastar SSE -----------------------------------------
// Visit /policies/permissions, wait for Datastar to open the SSE channel and
// apply the initial tbody patch. Then POST a row from the same browser
// (cookie auto-sent). The SSE channel should emit a fresh tbody patch and
// Datastar should swap it in -- the new action_code shows up in the DOM
// without a navigation.
//
// Login as admin first -- both the SSE connection's "can_edit" capture and
// the in-browser POST below need the session cookie.
await page.goto(`${BASE}/login`);
await page.selectOption("select[name=user_id]", "usr_admin_001");
await page.click("button[type=submit]");
await page.waitForURL(/\/me$/);

// Track SSE-related network so we can diagnose if the patch never arrives.
const sseReqs = [];
page.on("request", (req) => {
  if (req.url().includes("/policies/permissions/sse")) sseReqs.push(req.url());
});

await page.goto(`${BASE}/policies/permissions`);
// Datastar v1.0.1 fires data-init on mount; give it ~1s to open SSE + apply.
await page.waitForFunction(
  () => !!document.querySelector('#permissions-tbody tr'),
  { timeout: 5000 },
);
// Confirm Datastar actually opened the SSE channel before we test live updates.
// If this fails, it means data-init never fired -- the live-update check
// below would be diagnosing the wrong layer.
await new Promise((r) => setTimeout(r, 800));
check(
  "browser: Datastar opened SSE connection on data-init",
  sseReqs.length >= 1,
  `sse requests so far: ${sseReqs.length}`,
);

// Hook a window-side listener so we can see what Datastar/SSE events the
// page actually receives. Stored on window so the later evaluate can read it.
await page.evaluate(() => {
  window.__dsEvents = [];
  const types = ["datastar-fetch", "datastar-signal-patch", "datastar-prop-change"];
  for (const t of types) {
    document.addEventListener(t, (e) => {
      window.__dsEvents.push({
        t,
        // capture the type field of detail (e.g. datastar-patch-elements) and
        // the elements payload size so we can correlate with the POST below.
        kind: e.detail?.type,
        elementsLen: (e.detail?.argsRaw?.elements || "").length,
      });
    });
  }
});

// Wait for the INITIAL Datastar patch to arrive before triggering the edit.
// Server-side, `.cat -T <topic> --follow --new` only sees frames that arrive
// AFTER it subscribes; the initial patch's arrival proves the subscription
// is live. Without this, the POST below races the SSE handshake and the
// freshly-appended frame can be missed by the --new filter.
await page.waitForFunction(
  () => (window.__dsEvents || []).some(
    (e) => e.t === "datastar-fetch" && e.kind === "datastar-patch-elements",
  ),
  { timeout: 5000 },
);

const liveCode = `LIVE_BROWSER_E2E_${Date.now()}`;
await page.evaluate(async (code) => {
  await fetch('/policies/permissions', {
    method: 'POST',
    headers: { 'content-type': 'application/x-www-form-urlencoded' },
    body: `action_code=${code}&relation_code=PUBLIC&event_type_code=`,
    redirect: 'manual',
  });
}, liveCode);

// Wait for the SSE patch -- up to 3s.
let liveAppeared = false;
for (let i = 0; i < 30; i++) {
  liveAppeared = await page.evaluate(
    (code) => document.querySelector('#permissions-tbody')?.innerHTML.includes(code) || false,
    liveCode,
  );
  if (liveAppeared) break;
  await new Promise((r) => setTimeout(r, 100));
}
// Pull diagnostic events for failure detail.
const dsDiag = await page.evaluate(() => window.__dsEvents.slice(-8));
check(
  "browser: SSE pushed live row to /policies/permissions without reload",
  liveAppeared,
  `LIVE row ${liveCode} not in tbody within 3s; events=${JSON.stringify(dsDiag)}`,
);

// Clean up by looking up the row that holds OUR code, not "the last row".
// Same fragility lesson as the pre-browser block: a leftover row from a
// failed prior run would otherwise shift the index and clobber a real
// seed row.
await page.evaluate(async (code) => {
  const rows = Array.from(document.querySelectorAll('#permissions-tbody tr'));
  const idx = rows.findIndex((r) => r.querySelector('td code')?.textContent === code);
  if (idx >= 0) {
    await fetch('/policies/permissions/delete', {
      method: 'POST',
      headers: { 'content-type': 'application/x-www-form-urlencoded' },
      body: `row=${idx}`,
      redirect: 'manual',
    });
  }
}, liveCode);
// Give SSE a moment to apply the delete patch before navigating away.
await new Promise((r) => setTimeout(r, 300));

// Sign-out flow (now back on /policies/permissions, navigate to /me first).
await page.goto(`${BASE}/me`);
await page.click("button[type=submit]");  // sign-out form
await page.waitForURL(/\/$/);
const afterLogout = await page.locator("main").textContent();
check(
  "browser: post-logout / shows Not signed in",
  afterLogout.includes("Not signed in"),
  `after logout: ${afterLogout.slice(0, 80)}`,
);

check("browser: no JS errors on any page", pageErrors.length === 0, pageErrors.join("; "));

await browser.close();

if (failures.length > 0) {
  console.log(`\nFAILED: ${failures.length} check(s)`);
  process.exit(1);
}
console.log(`\nall ${failures.length === 0 ? "" : "non-failing "}checks ok`);
process.exit(0);
