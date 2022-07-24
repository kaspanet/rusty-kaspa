

# Parallel Block Processing

## Current sequential processing flow

Below we detail the current state of affairs in *go-kaspad* and discuss future parallelism opportunities. Processing dependencies between various stages are detailed in square brackets [***deps; type***].

### Header processing 

* Pre pow (aka "*header in isolation*" -- no DB writes to avoid spamming):
    * block version
    * timestamp not in future
    * parents limit (>0 AND <= limit)
* Pow:
    * parents not "virtual genesis"
    * parent headers exist [***headers; read***]
        * (returns either invalid parent error or missing parents list)
    * stage parents at all levels (stages topology manager; drops missing parents from level > 0; uses virtual genesis if no parents) [***relations; write***]
    * verify parents are antichain (reachability manager DAG queries) [***reachability; read***]
    * verify block is in pruning point future (uses reachability queries on parents) [***reachability; read***]
    * check pow of block (against block declared target)
    * check difficulty and blue work
        * run GHOSTDAG and stage [***reachability; read*** | ***ghostdag; write***]
        * calculate DAA window and stage; compute difficulty from window; [***windows; read | write***]
        * verify bits from calculated difficulty 
* Post pow (aka "*header in context*"):
    * validate median time (uses median time window) [***windows; read***]
    * check mergeset size limit (could be done following GHOSTDAG)
    * stage reachability data [***reachability; write***]
    * check indirect parents (level > 0) [***headers | relations | reachability; read***]
    * check bounded merge depth [***reachability; read | merge root store; write | finality store; write***]
    * check DAA score
    * check header blue work and blue score
    * validate header pruning point [***reachability | pruning store; read***]
* Commit all changes

### Block processing

* Block body in isolation:
    * verify all txs have utxo inputs
    * verify block merkle root
    * verify at least one tx
    * verify first tx is coinbase
    * verify all others are non-coinbase
    * check coinbase blue score
    * check txs are ordered by subnet ID
    * for each tx, validate tx in isolation (includes anything that can be checked w/o context)
    * check block mass
    * check if duplicate txs 
    * check double spends
    * validate gas limit

* Block body in context
    * check block is not pruned (reachability queries from all tips -- relies on reachability data of current block)
    * check all txs are finalized based on pov DAA score and median time
    * check parent bodies exist
    * check coinbase subsidy
* Stage and commit block body and block status


### Virtual-state processing (block UTXO data -- for context of chain blocks only)

* (*roughly*)
* build the utxo state for selected parent through utxo diffs from virtual
* build the utxo state for current block based on selected parent state and tx data from the mergeset 
* stage acceptance data
* update diff paths to virtual 
* update virtual state

## Parallel processing -- Discussion

There are two levels of possible concurrency to support: (i) process the various stages concurrently in a *pipeline*, i.e., when a block moves to body processing, other headers can enter the header processing stage, and so on; (ii) *parallelism* within each processing "station" of the pipeline, i.e., within header processing, allow *n* independent blocks to be processed in parallel. 

### Pipeline concurrency

The current code design (*go-kaspad*) already logically supports this since the various processing stages were already decoupled for supporting efficient IBD. 

### Header processing parallelism

If you analyze the dependency graph above you can see this is the most challenging part. For instance, we cannot create multiple staging areas in parallel, since committing them might introduce conflicts. 

I suggest we split the staging/writes during header processing into two categories: (i) writes that are append-only, meaning they only affect store data related to the currently processed block (for instance ghostdag data store, headers store, header status store, finality and merge root stores, windows stores -- all support this property); (ii) writes that modify state of other shared data (reachability reindexing, block relations children).

It seems to me that only DAG related write data is not append-only. So I suggest moving reachability and relations writes to a new processing unit named "Header DAG processing". This unit will support adding multiple blocks at one call to the reachability tree by performing a single reindexing for all (can be easily supported by current algos). 


### Block processing parallelism

Seems straightforward.


### Virtual processing parallelism

* Process each chain block + mergeset sequentially.
* Within each such step:
    * txs within each block can be validated against the utxo set in parallel 
    * blocks in the mergeset and txs within can be processed in parallel based on the consensus-agreed topological mergeset ordering -- however conflicts might arise and need to be taken care of according to said order.
