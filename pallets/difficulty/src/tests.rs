//! Tests for the fixed-anchor ASERT with break-only re-anchoring.

use crate::{mock::*, AnchorHeight, AnchorTimestamp, CurrentDifficulty, LastBlockTimestamp};
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
