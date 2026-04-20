# Pallet Difficulty

ASERT (Absolutely Scheduled Exponentially Rising Targets) difficulty adjustment for PoW consensus.

## Overview

This pallet dynamically adjusts the mining difficulty to keep block production close to the target interval (default: 20 seconds). Unlike recursive parent-based algorithms, ASERT computes each block's difficulty from a fixed anchor block using an exponential formula, eliminating cumulative rounding errors and feedback oscillation.

### Formula

```
next_target = anchor_target × 2^((time_delta - target_block_time × height_delta) / halflife)
```

- `anchor_target` — target value of the anchor block
- `time_delta` — current block timestamp minus anchor block timestamp (seconds)
- `height_delta` — current block height minus anchor block height
- `target_block_time` — ideal block interval (20s)
- `halflife` — 1800s (30 minutes)

### Key Properties

- **Absolute scheduling**: difficulty derived from anchor block, not recursively from parent
- **No cumulative error**: each computation is independent
- **Natural decay**: when blocks fall behind schedule, difficulty automatically decreases
- **Pure integer arithmetic**: 16-bit fixed-point with cubic polynomial 2^x approximation, no floating point

### Difficulty Decay (Hashrate Drop)

| Time Without Blocks | Difficulty |
|---------------------|------------|
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
    pub const TargetBlockTime: u64 = 20;      // seconds
    pub const DifficultyHalflife: u64 = 1800;  // seconds (30 minutes)
}

impl pallet_difficulty::Config for Runtime {
    type TargetBlockTime = TargetBlockTime;
    type Halflife = DifficultyHalflife;
}
```
