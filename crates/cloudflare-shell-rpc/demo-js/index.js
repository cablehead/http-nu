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
        return new Response(
          [
            "cloudflare-shell-rpc-demo-js",
            "",
            "Routes:",
            "  PUT    /fs/:ns/:path           -- writeFile (raw body bytes)",
            "  GET    /fs/:ns/:path           -- readFile (raw bytes back)",
            "  DELETE /fs/:ns/:path           -- rm (?recursive=1&force=1)",
            "  GET    /stat/:ns/:path         -- stat",
            "  GET    /list/:ns/:path         -- list",
            "  POST   /mkdir/:ns/:path        -- mkdir (?recursive=1)",
            "",
          ].join("\n"),
          { headers: { "content-type": "text/plain" } }
        );
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

      return new Response("not found\n", { status: 404 });
    } catch (e) {
      // Errors raised by the wasm-side RPC functions surface as thrown
      // JS Errors at the service-binding boundary. We surface the
      // POSIX-prefixed message verbatim (ENOENT: ..., EISDIR: ..., etc).
      return errResponse(e, 500);
    }
  },
};
