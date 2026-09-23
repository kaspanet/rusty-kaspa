# Kaspa WASM SDK Examples

Runnable examples for the Kaspa WASM SDK.

## Quickstart

**1. Build the SDK**

```bash
cd wasm
./build-release
```

This needs the Rust toolchain; see [Building from Source](../README.md#building-from-source)
in the parent `wasm/` folder for prerequisites and other build targets.

**2. Install and run a recipe**

```bash
cd wasm/examples
npm install
npx tsx recipes/keys/generate-mnemonic.ts
```

This needs the Node.js runtime; [install Node.js](https://nodejs.org/en/download) if you don't have it.

## Layout

| Path       | What's in it                                                                                                                             |
| ---------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `recipes/` | One concept each, grouped by domain (`keys/`, `rpc/`, `transactions/`, `wallet/`, `watch/`, `signing/`, `mining/`, `covenants/`, `zk/`). |
| `web/`     | The same SDK running in the browser.                                                                                                     |
| `shared/`  | Small helpers shared by the recipes.                                                                                                     |

## Language & running

Recipes are written in **TypeScript**; you can run them directly with [`tsx`](https://github.com/privatenumber/tsx):

```bash
npx tsx recipes/<area>/<name>.ts
```

Every call is typed by the shipped `kaspa.d.ts`; hover any symbol in your editor.

### Node version

Recipes that connect to RPC need **Node 22+**, which ships the global `WebSocket` the RPC client
uses.

## Running web examples

The `web/` examples run the same SDK in the browser. They must be served over
HTTP from the **root of the SDK folder** (`rusty-kaspa/wasm` when building from
source, or `kaspa-wasm32-sdk` in a release) — the examples use relative paths,
and WASM32 cannot be loaded over the `file://` protocol.

Any static web server works, run from the SDK root. Using Node (already
installed for the recipes):

```bash
npx http-server   # serves the current directory on http://localhost:8080
```

Or with the Rust equivalent:

```bash
cargo install http-server
http-server   # serves the current directory on http://localhost:7878
```

Then open `…/examples/web/index.html` on whichever port your server printed —
e.g. [http://localhost:8080/examples/web/index.html](http://localhost:8080/examples/web/index.html)
(npx) or [http://localhost:7878/examples/web/index.html](http://localhost:7878/examples/web/index.html)
(cargo).
