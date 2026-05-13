// JS consumer demo for cloudflare-shell-rpc.
//
// A tiny HTTP Worker that exercises every RPC method on the SHELL_FS
// service binding. Routes are intentionally curl-able so the smoke
// test (cf:fs:smoke) can hit them without speaking RPC itself.
//
// Routes:
//   GET  /                                      health / banner
//   GET  /fs/:namespace/*path                   readFile (returns bytes)
//   PUT  /fs/:namespace/*path                   writeFile (raw body bytes)
//   DELETE /fs/:namespace/*path?recursive&force rm
//   GET  /stat/:namespace/*path                 stat
//   GET  /list/:namespace/*path                 list (read_dir)
//   POST /mkdir/:namespace/*path?recursive      mkdir
//
// Wire shape (matches cloudflare-shell-rpc-types):
//   read/write -- bytes travel as base64-encoded `data` strings
//   stat/list  -- typed JS objects (kind: "file"|"directory"|"symlink", ...)
//   ack methods (write, mkdir, rm) -- {} on success

function parseFsPath(url, prefix) {
  // /<prefix>/<namespace>/<path...>  ->  { namespace, path }
  // path "/" is allowed (legal for list/stat on the root); rejected
  // for write/rm at the handler level if it doesn't make sense.
  //
  // url.pathname is NOT auto-decoded -- a path like
  //   /fs/alice/notes%2Fdraft.md
  // arrives here verbatim. We split on the first literal "/", then
  // decodeURIComponent the path tail so callers can reference paths
  // with embedded slashes (or any other URI-reserved char) by encoding.
  const stripped = url.pathname.slice(prefix.length);
  const slash = stripped.indexOf("/");
  if (slash < 0) return null;
  const namespace = stripped.slice(0, slash);
  let path;
  try {
    path = decodeURIComponent(stripped.slice(slash));
  } catch {
    return null;
  }
  if (!namespace || !path) return null;
  return { namespace, path };
}

function bytesToBase64(bytes) {
  let binary = "";
  for (let i = 0; i < bytes.length; i++) binary += String.fromCharCode(bytes[i]);
  return btoa(binary);
}

function base64ToBytes(b64) {
  const binary = atob(b64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

function jsonResponse(value, status = 200) {
  return new Response(JSON.stringify(value, null, 2), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function errResponse(e, status = 500) {
  const msg = e && e.message ? e.message : String(e);
  return new Response(msg + "\n", {
    status,
    headers: { "content-type": "text/plain" },
  });
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    const method = request.method;
    const path = url.pathname;

    try {
      if (path === "/") {
        return new Response(INDEX_HTML, {
          headers: { "content-type": "text/html; charset=utf-8" },
        });
      }

      // If the consumer has SHELL_FS_TOKEN configured (via wrangler vars
      // or a Secret), thread it through every RPC call. The server only
      // enforces this if its own SHELL_FS_TOKEN env var is set; otherwise
      // the field is ignored.
      const auth = env.SHELL_FS_TOKEN ?? undefined;

      // ── readFile / writeFile / rm under /fs/ ────────────────────────
      if (path.startsWith("/fs/")) {
        const parsed = parseFsPath(url, "/fs/");
        if (!parsed) return errResponse("usage: /fs/<namespace>/<path>", 400);
        const { namespace, path: fsPath } = parsed;

        if (method === "GET") {
          const resp = await env.SHELL_FS.readFile({ namespace, path: fsPath, auth });
          if (resp.data == null) return new Response("not found\n", { status: 404 });
          const bytes = base64ToBytes(resp.data);
          return new Response(bytes, {
            headers: { "content-type": "application/octet-stream" },
          });
        }

        if (method === "PUT") {
          const buf = new Uint8Array(await request.arrayBuffer());
          await env.SHELL_FS.writeFile({
            namespace,
            path: fsPath,
            data: bytesToBase64(buf),
            mimeType: request.headers.get("content-type") || undefined,
            auth,
          });
          return jsonResponse({ ok: true, bytes: buf.length });
        }

        if (method === "DELETE") {
          const recursive = url.searchParams.has("recursive");
          const force = url.searchParams.has("force");
          await env.SHELL_FS.rm({ namespace, path: fsPath, recursive, force, auth });
          return jsonResponse({ ok: true });
        }

        return errResponse(`method ${method} not allowed on /fs/`, 405);
      }

      // ── stat ────────────────────────────────────────────────────────
      if (path.startsWith("/stat/")) {
        const parsed = parseFsPath(url, "/stat/");
        if (!parsed) return errResponse("usage: /stat/<namespace>/<path>", 400);
        const resp = await env.SHELL_FS.stat({ ...parsed, auth });
        return jsonResponse(resp, resp.stat == null ? 404 : 200);
      }

      // ── list ────────────────────────────────────────────────────────
      if (path.startsWith("/list/")) {
        const parsed = parseFsPath(url, "/list/");
        if (!parsed) return errResponse("usage: /list/<namespace>/<path>", 400);
        const resp = await env.SHELL_FS.list({ ...parsed, auth });
        return jsonResponse(resp, resp.entries == null ? 404 : 200);
      }

      // ── mkdir ───────────────────────────────────────────────────────
      if (path.startsWith("/mkdir/")) {
        if (method !== "POST") return errResponse("mkdir is POST", 405);
        const parsed = parseFsPath(url, "/mkdir/");
        if (!parsed) return errResponse("usage: /mkdir/<namespace>/<path>", 400);
        const recursive = url.searchParams.has("recursive");
        await env.SHELL_FS.mkdir({ ...parsed, recursive, auth });
        return jsonResponse({ ok: true });
      }

      // ── lstat / exists / file_exists / readlink / realpath ──────────
      if (path.startsWith("/lstat/")) {
        const parsed = parseFsPath(url, "/lstat/");
        if (!parsed) return errResponse("usage: /lstat/<namespace>/<path>", 400);
        const resp = await env.SHELL_FS.lstat({ ...parsed, auth });
        return jsonResponse(resp, resp.stat == null ? 404 : 200);
      }

      if (path.startsWith("/exists/")) {
        const parsed = parseFsPath(url, "/exists/");
        if (!parsed) return errResponse("usage: /exists/<namespace>/<path>", 400);
        const resp = await env.SHELL_FS.exists({ ...parsed, auth });
        return jsonResponse(resp);
      }

      if (path.startsWith("/file_exists/")) {
        const parsed = parseFsPath(url, "/file_exists/");
        if (!parsed) return errResponse("usage: /file_exists/<namespace>/<path>", 400);
        const resp = await env.SHELL_FS.fileExists({ ...parsed, auth });
        return jsonResponse(resp);
      }

      if (path.startsWith("/readlink/")) {
        const parsed = parseFsPath(url, "/readlink/");
        if (!parsed) return errResponse("usage: /readlink/<namespace>/<path>", 400);
        const resp = await env.SHELL_FS.readlink({ ...parsed, auth });
        return jsonResponse(resp, resp.target == null ? 404 : 200);
      }

      if (path.startsWith("/realpath/")) {
        const parsed = parseFsPath(url, "/realpath/");
        if (!parsed) return errResponse("usage: /realpath/<namespace>/<path>", 400);
        const resp = await env.SHELL_FS.realpath({ ...parsed, auth });
        return jsonResponse(resp, resp.path == null ? 404 : 200);
      }

      // ── append / delete_file ────────────────────────────────────────
      if (path.startsWith("/append/")) {
        if (method !== "POST") return errResponse("append is POST", 405);
        const parsed = parseFsPath(url, "/append/");
        if (!parsed) return errResponse("usage: /append/<namespace>/<path>", 400);
        const buf = new Uint8Array(await request.arrayBuffer());
        await env.SHELL_FS.appendFile({
          ...parsed,
          data: bytesToBase64(buf),
          auth,
        });
        return jsonResponse({ ok: true, bytes: buf.length });
      }

      if (path.startsWith("/delete_file/")) {
        if (method !== "POST") return errResponse("delete_file is POST", 405);
        const parsed = parseFsPath(url, "/delete_file/");
        if (!parsed) return errResponse("usage: /delete_file/<namespace>/<path>", 400);
        const resp = await env.SHELL_FS.deleteFile({ ...parsed, auth });
        return jsonResponse(resp, resp.removed ? 200 : 404);
      }

      // ── cp / mv / symlink ───────────────────────────────────────────
      if (path.startsWith("/cp/")) {
        if (method !== "POST") return errResponse("cp is POST", 405);
        const parsed = parseFsPath(url, "/cp/");
        if (!parsed) return errResponse("usage: /cp/<namespace>/<src>?dst=<path>", 400);
        const dst = url.searchParams.get("dst");
        if (!dst) return errResponse("cp requires ?dst=<path>", 400);
        const recursive = url.searchParams.has("recursive");
        await env.SHELL_FS.cp({
          namespace: parsed.namespace,
          src: parsed.path,
          dst,
          recursive,
          auth,
        });
        return jsonResponse({ ok: true });
      }

      if (path.startsWith("/mv/")) {
        if (method !== "POST") return errResponse("mv is POST", 405);
        const parsed = parseFsPath(url, "/mv/");
        if (!parsed) return errResponse("usage: /mv/<namespace>/<src>?dst=<path>", 400);
        const dst = url.searchParams.get("dst");
        if (!dst) return errResponse("mv requires ?dst=<path>", 400);
        await env.SHELL_FS.mv({
          namespace: parsed.namespace,
          src: parsed.path,
          dst,
          auth,
        });
        return jsonResponse({ ok: true });
      }

      if (path.startsWith("/symlink/")) {
        if (method !== "POST") return errResponse("symlink is POST", 405);
        const parsed = parseFsPath(url, "/symlink/");
        if (!parsed) return errResponse("usage: /symlink/<namespace>/<linkPath>?target=<path>", 400);
        const target = url.searchParams.get("target");
        if (!target) return errResponse("symlink requires ?target=<path>", 400);
        await env.SHELL_FS.symlink({
          namespace: parsed.namespace,
          target,
          linkPath: parsed.path,
          auth,
        });
        return jsonResponse({ ok: true });
      }

      // ── glob / info (namespace-only routes) ─────────────────────────
      if (path === "/glob" || path.startsWith("/glob/")) {
        const namespace = path.slice("/glob".length).replace(/^\/|\/$/g, "");
        if (!namespace) return errResponse("usage: /glob/<namespace>?pattern=<glob>", 400);
        const pattern = url.searchParams.get("pattern");
        if (!pattern) return errResponse("glob requires ?pattern=<glob>", 400);
        const resp = await env.SHELL_FS.glob({ namespace, pattern, auth });
        return jsonResponse(resp);
      }

      if (path === "/info" || path.startsWith("/info/")) {
        const namespace = path.slice("/info".length).replace(/^\/|\/$/g, "");
        if (!namespace) return errResponse("usage: /info/<namespace>", 400);
        const resp = await env.SHELL_FS.workspaceInfo({ namespace, auth });
        return jsonResponse(resp);
      }

      return new Response("not found\n", { status: 404 });
    } catch (e) {
      // Errors raised by the wasm-side RPC functions surface as thrown
      // JS Errors at the service-binding boundary. We surface the
      // POSIX-prefixed message verbatim (ENOENT: ..., EISDIR: ..., etc).
      return errResponse(e, 500);
    }
  },
};

// Self-contained showcase UI served at GET /.
//
// Vanilla HTML/CSS/JS. Hits the same /fs/, /list/, /stat/, /mkdir/
// routes the rest of this Worker exposes, so the same auth + URL
// shape it shows JS consumers using. No CDN deps, no build step.
//
// Features: namespace switcher (live), file tree with breadcrumb
// nav + parent link, file viewer auto-rendering text / JSON /
// hex-of-binary, stat panel, drag-drop upload zone, mkdir + delete
// inline buttons. Errors surface as toast popups with the
// POSIX-prefixed messages from the server (ENOENT, EISDIR, ...).
const INDEX_HTML = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>cloudflare-shell-rpc · demo</title>
<style>
  :root {
    --bg: #0b0f14; --panel: #131923; --panel2: #1a212d;
    --text: #d7e0ec; --muted: #7d8a9b; --accent: #6ec1ff;
    --good: #6ee7b7; --bad: #fca5a5; --warn: #fcd34d;
    --border: #28313f; --hover: #1f2735;
    --mono: ui-monospace, "JetBrains Mono", "SF Mono", Menlo, monospace;
  }
  * { box-sizing: border-box; }
  html, body { margin: 0; padding: 0; height: 100%; background: var(--bg); color: var(--text); font: 14px/1.45 -apple-system, system-ui, sans-serif; }
  a { color: var(--accent); }
  header {
    padding: 14px 18px; border-bottom: 1px solid var(--border);
    background: var(--panel); display: flex; gap: 16px; align-items: center; flex-wrap: wrap;
  }
  header h1 { font-size: 14px; font-weight: 600; margin: 0; }
  header h1 .sub { color: var(--muted); font-weight: 400; }
  header .grow { flex: 1; }
  input, button, select {
    font: inherit; color: var(--text);
    background: var(--panel2); border: 1px solid var(--border);
    padding: 6px 10px; border-radius: 4px; outline: none;
  }
  input { font-family: var(--mono); }
  input:focus, select:focus { border-color: var(--accent); }
  button { cursor: pointer; }
  button:hover:not(:disabled) { background: var(--hover); border-color: var(--accent); }
  button.primary { background: var(--accent); color: #051226; border-color: var(--accent); font-weight: 600; }
  button.danger { color: var(--bad); border-color: #3a2330; }
  button.danger:hover { background: #2a161e; }
  button:disabled { opacity: 0.4; cursor: not-allowed; }
  main { display: grid; grid-template-columns: 320px 1fr; height: calc(100vh - 56px); }
  #tree-pane { background: var(--panel); border-right: 1px solid var(--border); overflow: auto; }
  #viewer-pane { display: flex; flex-direction: column; min-width: 0; }
  .toolbar {
    padding: 10px 14px; border-bottom: 1px solid var(--border);
    display: flex; gap: 8px; align-items: center; flex-wrap: wrap;
  }
  .toolbar .grow { flex: 1; }
  .crumb { font-family: var(--mono); color: var(--muted); font-size: 12px; padding: 0 14px 8px; }
  .crumb b { color: var(--text); }
  ul.entries { list-style: none; margin: 0; padding: 6px 0; }
  ul.entries li {
    padding: 6px 14px; cursor: pointer; font-family: var(--mono); font-size: 13px;
    display: flex; align-items: center; gap: 8px; border-left: 3px solid transparent;
  }
  ul.entries li:hover { background: var(--hover); }
  ul.entries li.selected { background: var(--hover); border-left-color: var(--accent); }
  ul.entries li .ico { width: 16px; opacity: 0.7; }
  ul.entries li .size { margin-left: auto; color: var(--muted); font-size: 11px; }
  ul.entries .empty { color: var(--muted); font-style: italic; }
  #drop {
    margin: 10px 14px 14px; padding: 14px;
    border: 1.5px dashed var(--border); border-radius: 6px;
    color: var(--muted); font-size: 12px; text-align: center;
  }
  #drop.over { border-color: var(--accent); color: var(--accent); background: var(--hover); }
  #viewer-stat {
    padding: 8px 14px; background: var(--panel2); border-bottom: 1px solid var(--border);
    font-family: var(--mono); font-size: 12px; color: var(--muted);
    display: flex; gap: 16px; flex-wrap: wrap;
  }
  #viewer-stat b { color: var(--text); font-weight: 500; }
  #viewer-content {
    flex: 1; overflow: auto; padding: 14px;
    font-family: var(--mono); font-size: 12px; white-space: pre-wrap;
    word-break: break-all;
  }
  #viewer-content.empty { color: var(--muted); font-style: italic; padding: 24px; }
  #toast {
    position: fixed; bottom: 16px; right: 16px; max-width: 420px;
    padding: 12px 16px; background: var(--panel); border: 1px solid var(--bad);
    color: var(--bad); font-family: var(--mono); font-size: 12px;
    border-radius: 6px; box-shadow: 0 4px 16px rgba(0,0,0,0.4);
    opacity: 0; transform: translateY(8px); transition: opacity .15s, transform .15s;
    pointer-events: none;
  }
  #toast.show { opacity: 1; transform: translateY(0); }
  .pill { font-size: 11px; padding: 2px 8px; border-radius: 999px; background: var(--panel2); color: var(--muted); }
  .pill.good { background: #0e2a1f; color: var(--good); }
  .footer { padding: 8px 14px; border-top: 1px solid var(--border); color: var(--muted); font-size: 11px; font-family: var(--mono); }
</style>
</head>
<body>

<header>
  <h1>cloudflare-shell-rpc <span class="sub">· demo-js</span></h1>
  <span class="pill" id="connstate">connecting…</span>
  <span class="pill" id="ws-info" title="workspace_info">·</span>
  <span class="grow"></span>
  <span style="color: var(--muted);">namespace:</span>
  <input id="ns-input" value="demo" spellcheck="false" style="width: 160px;">
  <button id="ns-switch">switch</button>
  <a href="https://github.com/joeblew999/http-nu/blob/joeblew999/crates/cloudflare-shell-rpc/README.md" target="_blank" style="color: var(--muted); text-decoration: none; font-size: 12px;">README ↗</a>
</header>

<main>
  <section id="tree-pane">
    <div class="toolbar">
      <button id="up-btn" disabled>↑ up</button>
      <button id="refresh-btn">↻</button>
      <button id="mkdir-btn">+ folder</button>
      <input id="glob-input" placeholder="glob (e.g. **/*.md)" spellcheck="false" style="flex: 1; min-width: 100px;">
      <button id="glob-btn" title="glob (clears with ↻)">find</button>
    </div>
    <div class="crumb" id="crumb">/</div>
    <ul class="entries" id="entries"><li class="empty">loading…</li></ul>
    <div id="drop">Drag &amp; drop files here to upload</div>
  </section>

  <section id="viewer-pane">
    <div class="toolbar">
      <span id="viewer-path" style="font-family: var(--mono); color: var(--muted);">no file selected</span>
      <span class="grow"></span>
      <button id="rename-btn" disabled title="mv">rename</button>
      <button id="download-btn" disabled>download</button>
      <button id="delete-btn" class="danger" disabled>delete</button>
    </div>
    <div id="viewer-stat" style="display: none;"></div>
    <pre id="viewer-content" class="empty">Click a file in the tree to view its contents.</pre>
    <div class="footer" id="footer">routes: <code>GET/PUT/DELETE /fs/:ns/:path</code> · <code>GET /stat/:ns/:path</code> · <code>GET /list/:ns/:path</code> · <code>POST /mkdir/:ns/:path</code></div>
  </section>
</main>

<div id="toast"></div>

<script>
"use strict";

const state = {
  ns: "demo",
  path: "/",        // current dir being listed
  selected: null,    // currently-selected file path (full)
};

const $ = (id) => document.getElementById(id);
const ENTRIES = $("entries"), CRUMB = $("crumb");
const UP_BTN = $("up-btn"), REFRESH = $("refresh-btn"), MKDIR = $("mkdir-btn");
const NS_INPUT = $("ns-input"), NS_SWITCH = $("ns-switch");
const VPATH = $("viewer-path"), VSTAT = $("viewer-stat"), VCONTENT = $("viewer-content");
const DOWNLOAD = $("download-btn"), DELETE = $("delete-btn"), RENAME = $("rename-btn");
const GLOB_INPUT = $("glob-input"), GLOB_BTN = $("glob-btn"), WS_INFO = $("ws-info");
const DROP = $("drop"), TOAST = $("toast"), CONN = $("connstate");

function toast(msg, kind = "error") {
  TOAST.textContent = msg;
  TOAST.style.borderColor = kind === "ok" ? "var(--good)" : "var(--bad)";
  TOAST.style.color = kind === "ok" ? "var(--good)" : "var(--bad)";
  TOAST.classList.add("show");
  clearTimeout(toast._t);
  toast._t = setTimeout(() => TOAST.classList.remove("show"), 3500);
}

function setConn(state) {
  CONN.textContent = state;
  CONN.classList.toggle("good", state === "connected");
}

function fmtSize(n) {
  if (n == null) return "";
  if (n < 1024) return n + " B";
  if (n < 1024 * 1024) return (n / 1024).toFixed(1) + " KB";
  return (n / 1024 / 1024).toFixed(2) + " MB";
}

function fmtDate(secs) {
  if (!secs) return "";
  return new Date(secs * 1000).toISOString().replace("T", " ").slice(0, 19);
}

async function api(method, urlPath, opts = {}) {
  const resp = await fetch(urlPath, { method, ...opts });
  if (!resp.ok) {
    const body = await resp.text();
    throw new Error(body || (resp.status + " " + resp.statusText));
  }
  return resp;
}

async function refreshTree() {
  CRUMB.innerHTML = "/" + state.path.split("/").filter(Boolean).map((seg, i, arr) => {
    const sub = "/" + arr.slice(0, i + 1).join("/");
    return \`<b><a href="#" data-path="\${sub}">\${seg}</a></b>\`;
  }).join("/") + (state.path === "/" ? "" : "/");
  UP_BTN.disabled = state.path === "/";

  try {
    const resp = await api("GET", \`/list/\${state.ns}\${state.path}\`);
    const body = await resp.json();
    const entries = body.entries ?? [];
    if (entries.length === 0) {
      ENTRIES.innerHTML = '<li class="empty">(empty)</li>';
    } else {
      entries.sort((a, b) => {
        if (a.kind !== b.kind) return a.kind === "directory" ? -1 : 1;
        return a.name.localeCompare(b.name);
      });
      ENTRIES.innerHTML = entries.map((e) => {
        const ico = e.kind === "directory" ? "📁" : "📄";
        return \`<li data-name="\${e.name}" data-kind="\${e.kind}"><span class="ico">\${ico}</span>\${e.name}</li>\`;
      }).join("");
    }
    setConn("connected");
  } catch (e) {
    ENTRIES.innerHTML = '<li class="empty">' + e.message + '</li>';
    setConn("error");
    toast(e.message);
  }
}

async function openFile(fullPath) {
  state.selected = fullPath;
  document.querySelectorAll("#entries li").forEach((li) => li.classList.remove("selected"));
  const sel = [...document.querySelectorAll("#entries li")].find((li) => li.dataset.name === fullPath.slice(state.path.length).replace(/^\\//, ""));
  if (sel) sel.classList.add("selected");

  VPATH.textContent = fullPath;
  VSTAT.style.display = "flex";
  DOWNLOAD.disabled = false;
  DELETE.disabled = false;
  RENAME.disabled = false;
  VCONTENT.classList.remove("empty");

  try {
    const statResp = await api("GET", \`/stat/\${state.ns}\${fullPath}\`);
    const stat = (await statResp.json()).stat;
    VSTAT.innerHTML = stat ? \`
      <span>kind: <b>\${stat.kind}</b></span>
      <span>size: <b>\${fmtSize(stat.size)}</b></span>
      <span>mime: <b>\${stat.mimeType}</b></span>
      <span>modified: <b>\${fmtDate(stat.modifiedAt)}</b></span>
    \` : "(no stat)";

    const resp = await api("GET", \`/fs/\${state.ns}\${fullPath}\`);
    const bytes = new Uint8Array(await resp.arrayBuffer());
    VCONTENT.textContent = renderBytes(bytes);
  } catch (e) {
    VCONTENT.textContent = e.message;
    toast(e.message);
  }
}

function renderBytes(bytes) {
  // Try utf-8 first; if it round-trips cleanly + only contains printable
  // chars / common whitespace, show as text. Otherwise show as a hex dump.
  try {
    const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    if (/^[\\x09\\x0a\\x0d\\x20-\\x7e\\u00a0-\\uffff]*$/.test(text)) {
      // Pretty-print JSON if possible
      try { return JSON.stringify(JSON.parse(text), null, 2); } catch {}
      return text;
    }
  } catch {}
  // Binary -> hex dump (first 1024 bytes)
  const max = Math.min(bytes.length, 1024);
  let out = "";
  for (let i = 0; i < max; i += 16) {
    const row = bytes.slice(i, i + 16);
    const hex = [...row].map((b) => b.toString(16).padStart(2, "0")).join(" ").padEnd(48);
    const ascii = [...row].map((b) => (b >= 32 && b < 127 ? String.fromCharCode(b) : ".")).join("");
    out += i.toString(16).padStart(8, "0") + "  " + hex + "  " + ascii + "\\n";
  }
  if (bytes.length > max) out += "... (" + (bytes.length - max) + " more bytes)";
  return out;
}

ENTRIES.addEventListener("click", (e) => {
  const li = e.target.closest("li");
  if (!li || !li.dataset.name) return;
  const name = li.dataset.name, kind = li.dataset.kind;
  const full = (state.path === "/" ? "/" : state.path + "/") + name;
  if (kind === "directory") {
    state.path = full;
    state.selected = null;
    refreshTree();
    clearViewer();
  } else {
    openFile(full);
  }
});

CRUMB.addEventListener("click", (e) => {
  const a = e.target.closest("a");
  if (!a) return;
  e.preventDefault();
  state.path = a.dataset.path;
  state.selected = null;
  refreshTree();
  clearViewer();
});

UP_BTN.addEventListener("click", () => {
  const parts = state.path.split("/").filter(Boolean);
  parts.pop();
  state.path = parts.length ? "/" + parts.join("/") : "/";
  state.selected = null;
  refreshTree();
  clearViewer();
});

REFRESH.addEventListener("click", () => {
  // Clearing the glob input alongside a refresh -- otherwise a stale
  // pattern silently filters the tree the next time refreshTree() runs
  // (we leave applyGlob in charge of the entries list when active).
  GLOB_INPUT.value = "";
  refreshTree();
});

// glob filter -- hits GET /glob/<ns>?pattern=... and renders the
// resulting flat list in place of the directory tree. Refresh (↻)
// clears the pattern and restores the tree.
async function applyGlob() {
  const pattern = GLOB_INPUT.value.trim();
  if (!pattern) { refreshTree(); return; }
  try {
    const resp = await api("GET", \`/glob/\${state.ns}?pattern=\${encodeURIComponent(pattern)}\`);
    const body = await resp.json();
    const paths = body.paths ?? [];
    CRUMB.innerHTML = \`<b>glob</b> · pattern <b>\${pattern}</b> · \${paths.length} match\${paths.length === 1 ? "" : "es"}\`;
    UP_BTN.disabled = true;
    if (paths.length === 0) {
      ENTRIES.innerHTML = '<li class="empty">(no matches)</li>';
    } else {
      ENTRIES.innerHTML = paths.map((p) => {
        const name = p.split("/").pop() || p;
        return \`<li data-glob-path="\${p}"><span class="ico">🔎</span>\${name} <span class="size">\${p}</span></li>\`;
      }).join("");
    }
    setConn("connected");
  } catch (e) { toast(e.message); }
}

GLOB_BTN.addEventListener("click", applyGlob);
GLOB_INPUT.addEventListener("keydown", (e) => { if (e.key === "Enter") applyGlob(); });

MKDIR.addEventListener("click", async () => {
  const name = prompt("new folder name (under " + state.path + "):");
  if (!name) return;
  const full = (state.path === "/" ? "/" : state.path + "/") + name;
  try {
    await api("POST", \`/mkdir/\${state.ns}\${full}\`);
    toast("created " + full, "ok");
    refreshTree();
  } catch (e) { toast(e.message); }
});

DELETE.addEventListener("click", async () => {
  if (!state.selected) return;
  if (!confirm("delete " + state.selected + "?")) return;
  try {
    await api("DELETE", \`/fs/\${state.ns}\${state.selected}\`);
    toast("deleted", "ok");
    clearViewer();
    refreshTree();
    refreshInfo();
  } catch (e) { toast(e.message); }
});

RENAME.addEventListener("click", async () => {
  if (!state.selected) return;
  const dst = prompt("rename " + state.selected + " to:", state.selected);
  if (!dst || dst === state.selected) return;
  try {
    await api("POST", \`/mv/\${state.ns}\${state.selected}?dst=\${encodeURIComponent(dst)}\`);
    toast("renamed -> " + dst, "ok");
    state.selected = dst;
    refreshTree();
    refreshInfo();
  } catch (e) { toast(e.message); }
});

// workspace_info -- aggregate counts for the current namespace.
// Refreshed on namespace switch and after every mutation so the
// header pill reflects whatever the server actually has.
async function refreshInfo() {
  try {
    const resp = await api("GET", \`/info/\${state.ns}\`);
    const info = (await resp.json()).info;
    WS_INFO.textContent = \`\${info.fileCount}f · \${info.directoryCount}d · \${fmtSize(info.totalBytes)}\`;
    WS_INFO.classList.add("good");
  } catch (e) {
    WS_INFO.textContent = "info error";
    WS_INFO.classList.remove("good");
  }
}

DOWNLOAD.addEventListener("click", () => {
  if (!state.selected) return;
  const a = document.createElement("a");
  a.href = \`/fs/\${state.ns}\${state.selected}\`;
  a.download = state.selected.split("/").pop();
  a.click();
});

NS_SWITCH.addEventListener("click", switchNs);
NS_INPUT.addEventListener("keydown", (e) => { if (e.key === "Enter") switchNs(); });

function switchNs() {
  const v = NS_INPUT.value.trim();
  if (!v) return;
  state.ns = v;
  state.path = "/";
  state.selected = null;
  GLOB_INPUT.value = "";
  clearViewer();
  refreshTree();
  refreshInfo();
}

function clearViewer() {
  VPATH.textContent = "no file selected";
  VSTAT.style.display = "none";
  VCONTENT.classList.add("empty");
  VCONTENT.textContent = "Click a file in the tree to view its contents.";
  DOWNLOAD.disabled = true;
  DELETE.disabled = true;
  RENAME.disabled = true;
}

// Drag-drop upload
["dragenter", "dragover"].forEach((ev) => DROP.addEventListener(ev, (e) => {
  e.preventDefault(); DROP.classList.add("over");
}));
["dragleave", "drop"].forEach((ev) => DROP.addEventListener(ev, (e) => {
  e.preventDefault(); DROP.classList.remove("over");
}));
DROP.addEventListener("drop", async (e) => {
  const files = [...(e.dataTransfer?.files ?? [])];
  if (files.length === 0) return;
  for (const f of files) {
    const full = (state.path === "/" ? "/" : state.path + "/") + f.name;
    try {
      await api("PUT", \`/fs/\${state.ns}\${full}\`, {
        headers: { "content-type": f.type || "application/octet-stream" },
        body: f,
      });
      toast("uploaded " + f.name + " (" + fmtSize(f.size) + ")", "ok");
    } catch (e) {
      toast("upload " + f.name + " failed: " + e.message);
    }
  }
  refreshTree();
  refreshInfo();
});

refreshTree();
refreshInfo();
</script>
</body>
</html>`;
