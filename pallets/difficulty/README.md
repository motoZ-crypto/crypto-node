# Pallet Difficulty

ASERT (Absolutely Scheduled Exponentially Rising Targets) difficulty adjustment for PoW consensus.

## Overview

This pallet dynamically adjusts the mining difficulty to keep block production close to the target interval (default: 20 seconds). It uses ASERT against a fixed anchor block (the original chain anchor), so each block's target is computed independently of intermediate blocks. When the gap between two consecutive blocks exceeds a configured threshold, the chain is considered to have resumed from an outage and the anchor is moved forward to the recovery block, so subsequent blocks are evaluated relative to the resumption point rather than dragging the long gap into every future computation.

### Formula

```
next_target = anchor_target × 2^((time_delta - target_block_time × height_delta) / halflife)
```

- `anchor_target` — target value of the anchor block
- `time_delta` — current block timestamp minus anchor block timestamp (seconds)
- `height_delta` — current block height minus anchor block height
- `target_block_time` — ideal block interval (20s)
- `halflife` — 1800s (30 minutes)

### Interruption recovery

In `on_finalize`, after computing the new difficulty for the just-finalized block N, the pallet checks the gap from block N-1's timestamp. If the gap exceeds `BreakThresholdSecs` (default 1800s), it re-anchors to block N: `AnchorTarget = next_target`, `AnchorTimestamp = ts(N)`, `AnchorHeight = N`. From the next block onward, ASERT evaluates against the resumed anchor with `Δh` restarting from 1, so the chain does not need to mine many catch-up blocks to "work off" the outage gap.

Miners continue to query `realtime_difficulty(now_secs)` which evaluates ASERT against the current anchor with wall-clock `Δt`. During an outage `Δt` grows and difficulty decays naturally, so the recovery block remains feasible to mine.

### Key Properties

- **Absolute scheduling**: difficulty derived from a fixed anchor while the chain is healthy.
- **No cumulative error**: each computation is independent of intermediate blocks.
- **Natural decay**: when blocks fall behind schedule, difficulty automatically decreases.
- **Interruption recovery**: anchor advances to the recovery block, eliminating the need to mine long bursts of catch-up blocks.

### Difficulty Decay (Hashrate Drop)

| Time Without Blocks | Difficulty |
| ------------------- | ---------- |
| 0                   | 100%       |
| 30 min              | 50%        |
| 1 hour              | 25%        |
| 2 hours             | 6.25%      |
| 4 hours             | 0.39%      |

## Runtime API

The pallet exposes `DifficultyApi` with two methods:

- `anchor_params()` — returns anchor target, timestamp, height, target block time, and halflife
- `realtime_difficulty(now_secs)` — computes difficulty using an external wall-clock timestamp, allowing miners to see real-time difficulty decay even when no blocks are being produced

## Configuration

```rust
parameter_types! {
    pub const TargetBlockTime: u64 = 20;                       // seconds
    pub const DifficultyHalflife: u64 = 1800;                  // seconds (30 minutes)
    pub const DifficultyBreakThresholdSecs: u64 = 1800;        // outage threshold (seconds)
}

impl pallet_difficulty::Config for Runtime {
    type TargetBlockTime = TargetBlockTime;
    type Halflife = DifficultyHalflife;
    type BreakThresholdSecs = DifficultyBreakThresholdSecs;
}
```
