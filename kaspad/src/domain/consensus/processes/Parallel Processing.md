

# Parallel Block Processing

## Current sequential processing flow

Below we detail the current state of affairs in *go-kaspad*. Processing dependencies between various stages is detailed in square brackets [***deps; type***].

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

<!-- ### Header DAG processing -->

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


### Virtual-state processing (block UTXO data -- for chain blocks context only)

* (*in short*)
* build the utxo state for selected parent through utxo diffs from virtual
* build the utxo change for current block based on tx data from the mergeset 
* stage acceptance data
* update diff paths to virtual 
* update virtual parents state