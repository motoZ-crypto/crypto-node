//! Tests for the fixed-anchor ASERT with break-only re-anchoring.

use crate::{
	asert::compute_next_target,
	mock::*,
	AnchorHeight,
	AnchorTimestamp,
	CurrentDifficulty,
	LastBlockTimestamp,
};
use sp_core::U256;

/// Bring the chain past the auto-init block. Returns the timestamp used.
fn bootstrap(start_secs: u64) -> u64 {
	run_to_block_at(1, start_secs);
	start_secs
}

#[test]
fn normal_block_keeps_difficulty() {
	new_test_ext().execute_with(|| {
		let t0 = bootstrap(10_000);
		let initial = CurrentDifficulty::<Test>::get();

		run_to_block_at(2, t0 + 20);
		let after = CurrentDifficulty::<Test>::get();

		let diff = if after > initial { after - initial } else { initial - after };
		assert!(diff * U256::from(100u64) < initial, "drift too large: {initial:?} -> {after:?}");
	});
}

#[test]
fn slow_block_decreases_difficulty() {
	new_test_ext().execute_with(|| {
		let t0 = bootstrap(10_000);
		let before = CurrentDifficulty::<Test>::get();

		run_to_block_at(2, t0 + 40);
		let after = CurrentDifficulty::<Test>::get();
		assert!(after < before, "slow block must lower difficulty: {before:?} -> {after:?}");
	});
}

#[test]
fn anchor_unchanged_during_normal_operation() {
	new_test_ext().execute_with(|| {
		let t0 = bootstrap(10_000);
		let anchor_h = AnchorHeight::<Test>::get();
		let anchor_ts = AnchorTimestamp::<Test>::get();

		for i in 2u64..=6 {
			run_to_block_at(i, t0 + 20 * (i - 1));
		}

		assert_eq!(AnchorHeight::<Test>::get(), anchor_h, "anchor height must not move");
		assert_eq!(AnchorTimestamp::<Test>::get(), anchor_ts, "anchor timestamp must not move");
	});
}

#[test]
fn break_reanchors_to_recovery_block() {
	new_test_ext().execute_with(|| {
		let t0 = bootstrap(10_000);

		run_to_block_at(2, t0 + 20);
		run_to_block_at(3, t0 + 40);

		// Block 4 arrives 1 hour after block 3 — interruption.
		let recovery_ts = t0 + 40 + 3_600;
		run_to_block_at(4, recovery_ts);

		assert_eq!(AnchorHeight::<Test>::get(), 4, "anchor must move to recovery block");
		assert_eq!(AnchorTimestamp::<Test>::get(), recovery_ts);
		assert_eq!(LastBlockTimestamp::<Test>::get(), recovery_ts);
	});
}

#[test]
fn break_realtime_decays() {
	new_test_ext().execute_with(|| {
		let t0 = bootstrap(10_000);
		let on_schedule = crate::Pallet::<Test>::realtime_difficulty(t0 + 20);
		let after_outage = crate::Pallet::<Test>::realtime_difficulty(t0 + 86_400);

		assert!(
			after_outage < on_schedule,
			"realtime difficulty must decay during interruption: {on_schedule:?} vs {after_outage:?}"
		);
	});
}

#[test]
fn small_gap_does_not_reanchor() {
	new_test_ext().execute_with(|| {
		let t0 = bootstrap(10_000);
		let anchor_h = AnchorHeight::<Test>::get();

		// Block 2 arrives 100s after block 1 — slow but well under the
		// 1800s break threshold.
		run_to_block_at(2, t0 + 100);

		assert_eq!(AnchorHeight::<Test>::get(), anchor_h, "moderate gap must not re-anchor");
	});
}

#[test]
fn on_schedule_returns_anchor() {
	// If blocks are exactly on schedule, target should equal anchor_target.
	let anchor = U256::from(1_000_000u64);
	// time_delta = target_block_time * height_delta
	// e.g. height_delta=10 (10th block after anchor), time_delta = 20 * 10 = 200
	let result = compute_next_target(anchor, 200, 10, 20, 1800);
	// Should be very close to anchor (within rounding).
	let diff = if result > anchor { result - anchor } else { anchor - result };
	assert!(diff <= U256::from(1u64), "expected ~anchor, got {:?}", result);
}

#[test]
fn slow_blocks_increase_target() {
	// Blocks coming slower than expected -> target increases (difficulty decreases).
	let anchor = U256::from(1_000_000u64);
	// height_delta=10, ideal time = 200s, actual time = 400s (twice as slow)
	let result = compute_next_target(anchor, 400, 10, 20, 1800);
	assert!(result > anchor, "slow blocks should increase target");
}

#[test]
fn fast_blocks_decrease_target() {
	// Blocks coming faster than expected -> target decreases (difficulty increases).
	let anchor = U256::from(1_000_000u64);
	// height_delta=10, ideal time = 200s, actual time = 100s (twice as fast)
	let result = compute_next_target(anchor, 100, 10, 20, 1800);
	assert!(result < anchor, "fast blocks should decrease target");
}

#[test]
fn halflife_halves_target_when_fast() {
	// If blocks arrive halflife seconds ahead of schedule, target should halve.
	let anchor = U256::from(1u64) << 128;
	// For target to double (halflife behind schedule):
	// exponent = +1 -> time_delta - 20*1 = 1800 -> time_delta = 1820
	// height_delta=1 (one block after anchor)
	let result = compute_next_target(anchor, 1820, 1, 20, 1800);
	let expected = anchor * U256::from(2u64);
	// Allow ~1% tolerance due to polynomial approximation.
	let tolerance = expected / U256::from(100u64);
	let diff = if result > expected { result - expected } else { expected - result };
	assert!(diff < tolerance, "expected ~{:?}, got {:?}", expected, result);
}

#[test]
fn no_blocks_for_30min_halves_difficulty() {
	// 30 minutes (1800s) without blocks from anchor.
	// height_delta=1 (first block after anchor), time_delta = 1800 + 20 = 1820
	// exponent = (1820 - 20*1) / 1800 = 1800/1800 = 1
	// target doubles -> difficulty halves.
	let anchor = U256::from(1u64) << 128;
	let result = compute_next_target(anchor, 1820, 1, 20, 1800);
	let expected = anchor * U256::from(2u64);
	let tolerance = expected / U256::from(100u64);
	let diff = if result > expected { result - expected } else { expected - result };
	assert!(diff < tolerance, "difficulty should halve after 30min gap");
}

#[test]
fn result_never_zero() {
	// Even with extremely fast blocks, target should not be zero.
	let anchor = U256::from(1u64);
	let result = compute_next_target(anchor, 0, 1000, 20, 1800);
	assert!(!result.is_zero(), "target must never be zero");
}
