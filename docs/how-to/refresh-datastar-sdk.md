# How to refresh the embedded Datastar SDK

A runbook for bumping the vendored Datastar JS bundle to a new upstream
version. There is no script for this; it is done by hand, and the steps below
exist because the version string is hardcoded in several places and one
post-download cleanup step (stripping the sourcemap line) is easy to forget.

This is written for whoever does the next bump (likely a future Claude session).
Follow it top to bottom.

## What gets vendored

- `src/stdlib/datastar/datastar@<ver>.js` -- the upstream bundle, fetched from
  jsdelivr.
- `src/stdlib/datastar/datastar@<ver>.js.br` -- a brotli-precompressed copy of
  the exact same bytes, served via `Content-Encoding: br` when the client
  accepts brotli.
- `src/stdlib/datastar/mod.nu` -- the hand-written Nushell SSE SDK module. This
  is independent of the JS bundle; only touch it if the SSE event contract
  (`datastar-patch-elements`, `datastar-patch-signals`, signal parsing, etc.)
  changes upstream. A version bump alone does not require editing it beyond the
  two version constants noted below.

Both `.js` and `.js.br` are pulled into the binary with `include_bytes!`, so a
rebuild is required after changing them.

## Where the version is hardcoded

All of these carry the literal version (e.g. `1.0.1`) and must change together:

| Location | What |
| --- | --- |
| `src/stdlib/datastar/datastar@<ver>.js` | bundle filename |
| `src/stdlib/datastar/datastar@<ver>.js.br` | brotli filename |
| `src/stdlib/datastar/mod.nu` | `DATASTAR_CDN_URL` and `DATASTAR_JS_PATH` consts |
| `src/handler.rs` | `DATASTAR_JS_PATH` const + both `include_bytes!` paths |
| `src/logging.rs` | `DATASTAR_VERSION` const (used in startup banner and version output) |

Grep to confirm you caught them all:

```bash
git grep -n '1\.0\.1'   # replace with the OLD version you are replacing
```

## Steps

Replace `OLD` and `NEW` below with the actual versions (e.g. `OLD=1.0.1`,
`NEW=1.0.2`).

1. **Fetch the new bundle** from jsdelivr (same source the `DATASTAR_CDN_URL`
   const points at, just with the new version):

   ```bash
   cd src/stdlib/datastar
   curl -fsSL \
     "https://cdn.jsdelivr.net/gh/starfederation/datastar@NEW/bundles/datastar.js" \
     -o "datastar@NEW.js"
   ```

2. **Strip the sourcemap comment.** Upstream ships the bundle with a trailing
   `//# sourceMappingURL=datastar.js.map` line. We do not ship the `.map`, so a
   browser that sees this line requests it and gets a 404. Remove the line:

   ```bash
   grep -v '^//# sourceMappingURL=' "datastar@NEW.js" > tmp && mv tmp "datastar@NEW.js"
   grep -c sourceMappingURL "datastar@NEW.js"   # must print 0
   ```

   (This is also why we serve no sourcemap header from `handler.rs`: there is
   nothing to point at. Keep it that way.)

3. **Regenerate the brotli copy** from the stripped `.js`, and verify it
   roundtrips to identical bytes:

   ```bash
   rm -f "datastar@NEW.js.br"
   brotli -q 11 -o "datastar@NEW.js.br" "datastar@NEW.js"
   brotli -d -c "datastar@NEW.js.br" | cmp - "datastar@NEW.js" && echo "br matches js"
   ```

4. **Remove the old files:**

   ```bash
   git rm "datastar@OLD.js" "datastar@OLD.js.br"
   ```

5. **Update the version string** in the three Rust/Nu locations from the table
   above (`mod.nu`, `handler.rs`, `logging.rs`). After editing, this grep should
   return nothing:

   ```bash
   cd ../../..        # back to repo root
   git grep -n 'OLD'  # the old version literal; expect no hits
   ```

6. **Build and verify.** A version mismatch or a missed rename surfaces as a
   `include_bytes!` "file not found" at compile time.

   ```bash
   ./scripts/check.sh
   ```

7. **Smoke test the served asset** -- confirm both the plain and brotli paths
   return 200 and the sourcemap line is gone:

   ```bash
   target/debug/http-nu --datastar 127.0.0.1:9931 -c '{|req| "ok"}' &
   SRV=$!; sleep 1
   curl -s http://127.0.0.1:9931/datastar@NEW.js | grep -c sourceMappingURL          # want 0
   curl -s -H 'Accept-Encoding: br' http://127.0.0.1:9931/datastar@NEW.js \
     | brotli -d -c | grep -c sourceMappingURL                                        # want 0
   kill $SRV
   ```

## Notes

- The brotli copy can be produced with any valid brotli encoder; the browser
  decodes it regardless of the encoder's params. `brotli -q 11` (max) is fine for
  a static asset. This is unrelated to the streaming brotli in
  `src/compression.rs`, which compresses responses on the fly.
- If upstream stops emitting the `sourceMappingURL` line, step 2 becomes a no-op
  (the grep prints 0 before you do anything) -- harmless, leave it in.
