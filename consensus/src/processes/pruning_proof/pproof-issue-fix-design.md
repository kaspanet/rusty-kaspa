

Problem desc: 

Level relation subdags can have missing links (due to pruning of intermediate blocks) and yet be reachable from pov of the reachability service (bcs it preserves reachability relation between any 2 non-pruned blocks) 


Long-term "correct" solution:

- Maintain the invariant the future(level-root) is always fully kept, and bound the search for next root to be within this future area
- This will solve the problem bcs we will always be searching within a non-pruned area where level-relations correlate with global reachability 

Complexities with this solution and why it's not simple to do now:

- Need to make sure we keep the level past of cut(pp)** \cap future(level-root) and not only the level root-tip diamond.
- Need to verify the above is true when receiving new proofs, but old version nodes will not send this data

**see crate level docs in lib.rs 


Current solution:

- Except this gap and build per level reachability stores along with the GD population
- When building from descriptor populate headers in a following bottom-up traversal (use top-down to populate relations and then populate headers when going back up. the dual traversal enforces correct level reachability access)


Details:

- Follow validate.rs and maintain a similar reachability store+service within populate_level_proof_ghostdag_data
