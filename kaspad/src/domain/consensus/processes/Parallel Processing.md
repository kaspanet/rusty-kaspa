

# Parallel Block Processing

## Current sequential processing flow

### Header processing 

* Pre pow (aka "*header in isolation*" -- no DB writes to avoid spamming):
    * block version
    * timestamp not in future
    * parents limit (>0 AND <= limit>)
* Pow:
    * parents not "virtual genesis"
    * parent headers exist 
        * (returns either invalid parent error or missing parents list)
    * set parents at all levels (stages topology manager)
    * verify parents are antichain (reachability manager DAG queries)
    * verify block is in pruning point future (uses reachability queries on parents) 
    * check pow of block (against block declared target)
    * check difficulty and blue work
        * run GHOSTDAG and stage
        * calculate DAA window and stage
        * verify bits from calculated difficulty 
* Post pow (aka "*header in context*"):
    * validate median time (uses median time window)
    * check mergeset size limit (could be done following GHOSTDAG)
    * stage reachability data
    * check indirect parents (level > 0)
    * check bounded merge depth 
    * check DAA score
    * check header blue work and blue score
    * validate header pruning point  







<!-- ### Header DAG processing -->

### Block processing (in isolation)

### Virtual-state processing (block data in context -- for chain blocks)