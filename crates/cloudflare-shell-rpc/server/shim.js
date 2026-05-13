// Custom shim template for cloudflare-shell-rpc-server.
//
// Why this file exists: worker-build's default (module) shim wraps
// `fetch`/`queue`/`scheduled` with `exports.fetch.call(this, request,
// this.env, this.ctx)`, but for arbitrary `#[wasm_bindgen]` exports
// it emits bare `Entrypoint.prototype.foo = imports.foo;` -- those
// functions never see `this.env` inside their wasm body, so they
// can't reach DurableObject / R2 / KV bindings.
//
// Until upstream wasm-bindgen PR #4757 lands (referenced by a TODO in
// worker-build itself), the workaround is to own the WorkerEntrypoint
// class ourselves and explicitly pass `this.env` into each RPC method.
//
// This file is the LEGACY-mode shim template: worker-build's
// main_legacy.rs reads it (via the `CUSTOM_SHIM` env var set in
// mise.toml's `cf:fs:build` task), substitutes the `$SNIPPET_*` /
// `$WAIT_UNTIL_RESPONSE` placeholders, then esbuilds it to
// `build/worker/shim.mjs` -- which is what wrangler.toml points at.
//
// worker-build's main_legacy substitutes three placeholders below
// before esbuild runs: SNIPPET_JS_IMPORTS (empty here -- no wbg JS
// snippets), SNIPPET_WASM_IMPORTS (same), WAIT_UNTIL_RESPONSE (empty
// unless RUN_TO_COMPLETION is set). Don't reference the $-prefixed
// names in comments -- str::replace would substitute those too and
// the resulting comment breaks esbuild's parser.
//
// Adding a new RPC method: declare it in src/rpc.rs with
// `#[wasm_bindgen(js_name = ...)]`, then add a one-liner below
// mirroring the existing pattern. The wasm-bindgen export shows up on
// `imports` automatically.

import * as imports from "./index_bg.js";
export * from "./index_bg.js";
import wasmModule from "./index.wasm";
import { WorkerEntrypoint } from "cloudflare:workers";
$SNIPPET_JS_IMPORTS

const instance = new WebAssembly.Instance(wasmModule, {
    "./index_bg.js": imports,
    $SNIPPET_WASM_IMPORTS
});

imports.__wbg_set_wasm(instance.exports);
instance.exports.__wbindgen_start?.();

export { wasmModule };

class Entrypoint extends WorkerEntrypoint {
    // HTTP entrypoint. Health probe; #[event(fetch)] returns "shell-fs-rpc OK".
    async fetch(request) {
        let response = imports.fetch(request, this.env, this.ctx);
        $WAIT_UNTIL_RESPONSE;
        return await response;
    }

    // RPC methods. Each forwards `(this.env, args)` to the wasm side.
    async readFile(args)      { return await imports.readFile(this.env, args);      }
    async writeFile(args)     { return await imports.writeFile(this.env, args);     }
    async stat(args)          { return await imports.stat(this.env, args);          }
    async mkdir(args)         { return await imports.mkdir(this.env, args);         }
    async rm(args)            { return await imports.rm(this.env, args);            }
    async list(args)          { return await imports.list(this.env, args);          }
    async exists(args)        { return await imports.exists(this.env, args);        }
    async lstat(args)         { return await imports.lstat(this.env, args);         }
    async appendFile(args)    { return await imports.appendFile(this.env, args);    }
    async cp(args)            { return await imports.cp(this.env, args);            }
    async mv(args)            { return await imports.mv(this.env, args);            }
    async symlink(args)       { return await imports.symlink(this.env, args);       }
    async readlink(args)      { return await imports.readlink(this.env, args);      }
    async realpath(args)      { return await imports.realpath(this.env, args);      }
    async glob(args)          { return await imports.glob(this.env, args);          }
    async fileExists(args)    { return await imports.fileExists(this.env, args);    }
    async deleteFile(args)    { return await imports.deleteFile(this.env, args);    }
    async workspaceInfo(args) { return await imports.workspaceInfo(this.env, args); }
}

export default Entrypoint;
