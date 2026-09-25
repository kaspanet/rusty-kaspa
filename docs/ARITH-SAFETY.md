# Arithmetic overflow policy

## Policy

The workspace enables Clippy's `arithmetic_side_effects` lint to make potentially overflowing arithmetic explicit. This policy covers production full-node code and RK Stratum, except for consensus and UTXO-index code. Test code (including `#[cfg(test)]` modules and test-only targets), benchmarks, examples, fuzz targets, and other non-production tooling are excluded. For the purposes of this policy, the `kaspa-database`, `kaspa-utils`, and `kaspa-perf-monitor` crates are technically part of consensus and are included in the consensus exemption. Arithmetic overflow in consensus or UTXO-index code is expected to panic rather than continue execution with a potentially invalid consensus result or corrupted UTXO-index state.

When the lint reports an operation, first determine whether it exposes an error in the code. Depending on the operation's domain semantics, choose one of the following resolutions:

- **a.** Fix the code if the arithmetic is incorrect.
- **b.** If the operation is safe because of a local invariant, add `#[allow(clippy::arithmetic_side_effects, reason = "...")]` as narrowly as possible and document that invariant in the `reason` string.
- **c.** Return an error when overflow occurs.
- **d.** Use a saturating operation when saturation is the correct behavior.


Keep allowances small enough that a reader can see every covered arithmetic operation and verify the reason without searching through a large block. An allowance on a single statement or short, cohesive block is preferred. If several operations rely on different invariants, give each operation its own allowance and reason.

## Safety reasons

Use a specific explanation when safety depends on a bound, ordering check, nonzero divisor, short-circuit condition, or another local invariant:

```rust
#[allow(clippy::arithmetic_side_effects, reason = "The branch above guarantees remaining > 0.")]
{
    remaining -= 1;
}
```

The following collection and counter shorthands may only be used when every variable participating in the arithmetic has type `usize`, `isize`, `u64`, or `i64`:

- `ARITH-SAFETY(INDEX)` means the value is an item position or iterator cursor. Such a value is expected to remain far below `usize::MAX`.
- `ARITH-SAFETY(LENGTH)` means the value is a collection length or size derived from actual items, rather than accepted as a trusted numeric claim. It is assumed to remain far below the relevant 64-bit or pointer-sized limit.
- `ARITH-SAFETY(COUNTER)` means the value counts objects, events, subscriptions, attempts, processed items, or similar entities. Counts may advance one at a time or by the size of an actual batch, and may be summed across collections. We expect `count << usize::MAX`, meaning "much smaller than" (not a bit shift), and likewise well below the maximum of the actual counter type.

These shorthands express a deliberate operational assumption, not a mathematical proof. Do not use them for other integer types, arithmetic on arbitrary input numbers, monetary values, protocol fields, or any value whose numeric bound merely comes from a caller. Small counts alone do not justify subtraction that could underflow, division by zero, or arbitrary multiplication. Document the concrete invariant, correct the operation, or leave the lint unresolved.

`ARITH-SAFETY(TIMESTAMP)` applies to real, locally recorded 64-bit integer timestamps, such as milliseconds sampled with `unix_now()`. We expect `ts << u64::MAX` (and below `i64::MAX` for signed timestamps). It also applies to an `Instant` sampled from the local monotonic clock with `Instant::now()`, or derived from such an instant by adding bounded, operationally reasonable durations. Although the range of `Instant` is platform-dependent, we expect a correctly initialized local instant to have sufficient headroom for such durations. Trace the value to the local clock; a timestamp submitted by a peer, RPC caller, or another external source does not qualify merely because it has been stored locally. Any added interval must also be bounded. This shorthand does not establish timestamp ordering for subtraction, justify an unrestricted duration, or establish the range of an opaque `SystemTime` representation. A suffix can identify the clock source and interval:

```rust
#[allow(
    clippy::arithmetic_side_effects,
    reason = "ARITH-SAFETY(TIMESTAMP): last_checked_time comes from unix_now(); the interval is 10 seconds."
)]
let next_check_time = last_checked_time + 10_000;

#[allow(
    clippy::arithmetic_side_effects,
    reason = "ARITH-SAFETY(TIMESTAMP): last_scan was initialized with Instant::now(); the interval is 10 seconds."
)]
let next_scan = last_scan + Duration::from_secs(10);
```

Place the allowance directly on the arithmetic expression when that is readable:

```rust
#[allow(clippy::arithmetic_side_effects, reason = "ARITH-SAFETY(INDEX)")]
index += 1;

#[allow(clippy::arithmetic_side_effects, reason = "ARITH-SAFETY(COUNTER)")]
active_subscriptions += 1;

#[allow(clippy::arithmetic_side_effects, reason = "ARITH-SAFETY(COUNTER)")]
count += batch.len() as u64;

#[allow(clippy::arithmetic_side_effects, reason = "ARITH-SAFETY(COUNTER): chunk_len is the actual batch length as u64.")]
lanes_sent += chunk_len;
```

When Clippy requires an attribute on an enclosing statement or block, keep that scope narrow:

```rust
#[allow(clippy::arithmetic_side_effects, reason = "ARITH-SAFETY(INDEX)")]
{
    index += 1;
}
```
