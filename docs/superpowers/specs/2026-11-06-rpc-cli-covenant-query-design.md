# RPC CLI covenant-query merge design

## Intent

Bring `master` into `non-interactive-cli` and expose the two newly available node
RPC methods through the non-interactive `kaspa-rpc` client: transaction retrieval
by accepting block and paginated UTXO retrieval by covenant ID.

## Goals

- Merge local `master` into the current `non-interactive-cli` branch.
- Add typed CLI subcommands and generic `call` dispatch for
  `get-transactions-by-accepting-block` and `get-utxos-by-covenant-ids`.
- Preserve all fields of the corresponding RPC request types, including accepting
  block selector, transaction IDs, covenant IDs, filters, pagination cursor, and
  budget.
- Prove the CLI command surface accepts representative invocations and uses the
  matching RPC request type.

## Non-goals

- Change the RPC-core, gRPC, wRPC, daemon, or covenant-index implementations
  already supplied by `master`.
- Change existing user files (`audit.txt`, `pr-1152-replies.md`, and pre-existing
  `docs/superpowers` content).

## Approach

1. **Merge `master`, then add typed subcommands and raw-call routing** (chosen):
   follows the current CLI structure, provides discoverable help, and keeps JSON
   automation available through `call`.
2. **Merge only and require generic transport use outside this CLI**: no client
   work, but does not make the branch support the methods.
3. **Add only generic `call` mappings**: smaller surface, but the complex cursor
   and accepting-block arguments would be needlessly opaque for interactive CLI
   use.

## Constraints

- Work directly on the user-approved current branch; do not touch unrelated
  untracked files.
- Maintain the project’s clap/typed-request command patterns and no new
  dependencies.
- The covenant query requires nodes started with `--utxoindex --covenant-index`.
- The accepting-block transaction query accepts 1–8192 unique transaction IDs;
  accepting block may be selected by hash or DAA score.
