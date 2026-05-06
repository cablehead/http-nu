# Browser tests

End-to-end tests that drive a real chromium against an isolated `http-nu`
instance. Useful for verifying client-side wiring (Datastar bindings, key
handling, `fetch` chains) that pure server-side smoke tests can't cover.

## Setup

```bash
cd tests-browser && npm install
```

Uses `playwright-core` (no bundled browser) + the system chromium at
`/usr/bin/chromium`.

## Run

From the repo root, after a debug build (`cargo build`):

```bash
node tests-browser/2048.test.mjs
```

Each test file spawns its own `http-nu` on a unique port.
