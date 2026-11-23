# RPC CLI covenant-query Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Merge `master` and let the non-interactive RPC CLI invoke the accepting-block transaction and covenant-ID UTXO query RPCs.

**Architecture:** Merge the backend API changes from `master` unchanged. Extend `rpc/cli` with typed clap subcommands that build the upstream request models, and register the same models in the JSON `call` command’s operation dispatch.

**Tech Stack:** Rust, clap, `kaspa-rpc-core`, async RPC client traits, cargo tests.

**Spec:** `docs/superpowers/specs/2026-11-06-rpc-cli-covenant-query-design.md`

## Global Constraints

- Work on the user-approved current branch and preserve unrelated untracked files.
- Do not alter the RPC backend implementations supplied by `master`.
- Add no dependencies; follow existing typed `rpc/cli` command patterns.
- Covenant queries expose all request fields and document the required `--utxoindex --covenant-index` node flags.
- Accepting-block transaction queries expose hash/DAA-score selection and transaction ID lists.

## Review Focus

- Accepting-block hash versus DAA-score selector serializes into the correct tagged RPC model; test the owning command/request construction.
- Multiple transaction IDs retain their input order and are not silently collapsed; test the request sent by the command.
- Optional covenant filters and cursor fields survive command parsing; test a fully populated request.
- Generic `call` names resolve to the correct request/trait method instead of reporting an unsupported method; test the command dispatch boundary.
- Existing CLI command names and raw-call routes remain intact; run the complete `kaspa-rpc-cli` test suite.

### Task 1: Merge master into the current branch

**Files:**
- Modify: files selected by Git’s merge, expected to include workspace dependency metadata and RPC-core API surfaces

**Interfaces:**
- Consumes: local `master` at `4863866fc`
- Produces: `kaspa_rpc_core::{GetTransactionsByAcceptingBlockRequest, GetUtxosByCovenantIdsRequest}` and their `RpcApi` calls for the CLI

- [ ] **Step 1: Verify the baseline CLI package builds and tests**

Run: `cargo test -p kaspa-rpc-cli`
Expected: PASS before the merge.

- [ ] **Step 2: Merge `master` with a merge commit**

Run: `git merge --no-ff master`
Expected: the current branch contains `master` without modifying unrelated untracked files.

- [ ] **Step 3: Build the CLI package after the merge**

Run: `cargo build -p kaspa-rpc-cli`
Expected: PASS.

### Task 2: Add accepting-block transaction CLI support

**Files:**
- Create: `rpc/cli/src/commands/get_transactions_by_accepting_block.rs`
- Modify: `rpc/cli/src/cli.rs`, `rpc/cli/src/commands.rs`, `rpc/cli/src/commands/call.rs`, and focused CLI tests

**Interfaces:**
- Consumes: `GetTransactionsByAcceptingBlockRequest`, `RpcAcceptingBlock`, and `RpcApi::get_transactions_by_accepting_block_call`
- Produces: `get-transactions-by-accepting-block` typed command and raw `call` operation

- [ ] **Step 1: Write a failing test for a typed accepting-block request**

Test a DAA-score selector and two literal transaction IDs; assert the constructed request has the score selector and IDs in input order. The test catches a command that omits or misclassifies the selector/IDs.

- [ ] **Step 2: Run the focused test to verify it fails**

Run: `cargo test -p kaspa-rpc-cli accepting_block -- --nocapture`
Expected: FAIL because the typed command/request construction does not exist.

- [ ] **Step 3: Implement the typed command and raw `call` route**

Construct `GetTransactionsByAcceptingBlockRequest` and invoke `get_transactions_by_accepting_block_call`; add the clap variant/module and the JSON dispatch case, matching existing CLI commands.

- [ ] **Step 4: Run the focused test to verify it passes**

Run: `cargo test -p kaspa-rpc-cli accepting_block -- --nocapture`
Expected: PASS.

### Task 3: Add covenant-ID UTXO CLI support

**Files:**
- Create: `rpc/cli/src/commands/get_utxos_by_covenant_ids.rs`
- Modify: `rpc/cli/src/cli.rs`, `rpc/cli/src/commands.rs`, `rpc/cli/src/commands/call.rs`, and focused CLI tests

**Interfaces:**
- Consumes: `GetUtxosByCovenantIdsRequest` and `RpcApi::get_utxos_by_covenant_ids_call`
- Produces: `get-utxos-by-covenant-ids` typed command and raw `call` operation

- [ ] **Step 1: Write a failing test for a fully populated covenant request**

Use literal covenant IDs, script-public-key filter, DAA range, cursor, and budget; assert every request field survives parsing/construction. The test catches a command that drops optional pagination/filter state.

- [ ] **Step 2: Run the focused test to verify it fails**

Run: `cargo test -p kaspa-rpc-cli covenant_ids -- --nocapture`
Expected: FAIL because the command/request construction does not exist.

- [ ] **Step 3: Implement the typed command and raw `call` route**

Construct `GetUtxosByCovenantIdsRequest` and invoke `get_utxos_by_covenant_ids_call`; add the clap variant/module and JSON dispatch case, including the node-index requirement in help text.

- [ ] **Step 4: Run the focused test to verify it passes**

Run: `cargo test -p kaspa-rpc-cli covenant_ids -- --nocapture`
Expected: PASS.

### Task 4: Verify merged CLI behavior

**Files:**
- Modify: any command documentation/help text required by Tasks 2–3

**Interfaces:**
- Consumes: both new typed commands and raw dispatch routes
- Produces: verified build and regression suite

- [ ] **Step 1: Run formatting and the complete CLI package tests**

Run: `cargo fmt --check && cargo test -p kaspa-rpc-cli`
Expected: PASS.

- [ ] **Step 2: Inspect the final diff and status**

Run: `git diff --check && git status --short`
Expected: no whitespace errors and unrelated untracked files remain untouched.
