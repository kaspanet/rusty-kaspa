# Parallel Block Processing

A design document intended to guide the new concurrent implementation of header and block processing.

## Sequential processing flow (in go-kaspad)

Below we detail the current state of affairs in _go-kaspad_ and discuss future parallelism opportunities. Processing dependencies between various stages are detailed in square brackets [***deps; type***].

### Header processing

- Pre-POW (aka "_header in isolation_" -- no DB writes to avoid spamming):
  - block version
  - timestamp not in future
  - parents limit (>0 AND <= limit)
- POW:
  - parents not "virtual genesis"
  - parent headers exist [***headers; read***]
    - (returns either invalid parent error or missing parents list)
  - stage parents at all levels (stages topology manager; drops missing parents from level > 0; uses virtual genesis if no parents) [***relations; write***]
  - verify parents are antichain (reachability manager DAG queries) [***reachability; read***]
  - verify block is in pruning point future (uses reachability queries on parents) [***reachability; read***]
  - check POW of block (against block declared target)
  - check difficulty and blue work
    - run GHOSTDAG and stage [***reachability; read*** | ***ghostdag; write***]
    - calculate DAA window and stage; compute difficulty from window; [***windows; read | write***]
    - verify bits from calculated difficulty
- Post-POW (aka "_header in context_"):
  - validate median time (uses median time window) [***windows; read***]
  - check mergeset size limit (could be done following GHOSTDAG)
  - stage reachability data [***reachability; write***]
  - check indirect parents (level > 0) [***headers | relations | reachability; read***]
  - check bounded merge depth [***reachability; read | merge root store; write | finality store; write***]
  - check DAA score
  - check header blue work and blue score
  - validate header pruning point [***reachability | pruning store; read***]
- Commit all changes

### Block processing

- Block body in isolation:

  - verify all transactions have UTXO inputs
  - verify block Merkle root
  - verify at least one transaction
  - verify first transaction is coinbase
  - verify all others are non-coinbase
  - check coinbase blue score
  - check transactions are ordered by subnet ID
  - for each transaction, validate it in isolation (includes anything that can be checked without context)
  - check block mass
  - check for duplicate transactions
  - check double spends
  - validate gas limit

- Block body in context:
  - check block is not pruned (reachability queries from all tips -- relies on reachability data of current block)
  - check all transactions are finalized based on PoV DAA score and median time
  - check parent bodies exist
  - check coinbase subsidy
- Stage and commit block body and block status

### Virtual-state processing (block UTXO data -- for the context of chain blocks only)

- (_roughly_)
- build the UTXO state for selected parent through UTXO diffs from virtual
- build the UTXO state for the current block based on selected parent state and transaction data from the mergeset
- stage acceptance data
- update diff paths to virtual
- update virtual state

## Parallel processing -- Discussion

There are two levels of possible concurrency to support: (i) process the various stages concurrently in a _pipeline_, i.e., when a block moves to body processing, other headers can enter the header processing stage, and so on; (ii) _parallelism_ within each processing "station" of the pipeline, i.e., within header processing, allowing _n_ independent blocks to be processed in parallel.

### Pipeline concurrency

The current code design (_go-kaspad_) already logically supports this since the various processing stages were already decoupled for supporting efficient IBD.

### Header processing parallelism

If you analyze the dependency graph above, you can see this is the most challenging part. For instance, we cannot easily create multiple staging areas in parallel, since committing them without synchronization will introduce logical write conflicts.

#### **Natural DAG parallelism**

Throughout header processing, the computation naturally depends on previous output from parents and ancestors of the currently processed header. This means we cannot concurrently process a block with its ancestors, however, we can concurrently process blocks that are parallel to each other in the DAG structure (i.e., blocks which are in the anticone of each other). As we increase block rate, more blocks will be mined in parallel — thus creating more parallelism opportunities as well.

This logic is already implemented in the `pipeline::HeaderProcessor` struct. The code uses a simple DAG-dependency mechanism to delay processing tasks until all depending tasks are completed. If there are no dependencies, a `rayon::spawn` assigns a thread-pool worker to the ready-to-be processed header.

#### **Managing store writes**

Most of the DB writes during header processing are append-only. That is, a new item is inserted into the store for the new header, and it is never modified in the future. This semantic means that no lock is needed to write to such a store as long as we verify that only a single worker thread "owns" each header (`DbGhostdagStore` is an example; note that the DB and cache instances used therein already support concurrency).

There are two exceptions to this: reachability and relations stores are both non-append-only. We currently assume that their processing time is negligible compared to overall header processing and thus use serialized upgradable-read/write locks to manage this part. See `pipeline::HeaderProcessor::commit_header`.

The current design should be benchmarked when header processing is fully implemented. If the reachability algorithms are a bottleneck, we can consider moving reachability and relations writes to a new processing unit named "Header DAG processing". This unit will support adding multiple blocks at one call to the reachability tree by performing a single reindexing for all (can be easily supported by current algorithms).

### Block processing parallelism

Seems straightforward.

### Virtual processing parallelism

- Process each chain block + mergeset sequentially.
- Within each such step:
  - transactions within each block can be validated against the UTXO set in parallel
  - blocks in the mergeset and transactions within them can be processed in parallel based on the consensus-agreed topological mergeset ordering -- however, conflicts might arise and need to be resolved according to said order.
