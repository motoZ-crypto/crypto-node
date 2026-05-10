//! Tests for the fixed-anchor ASERT with break-only re-anchoring.

use crate::{
	mock::*,
	AnchorHeight,
	AnchorTimestamp,
	CurrentDifficulty,
	LastBlockTimestamp,
};
use frame_support::traits::Get;

#[test]
fn normal_block_keeps_difficulty() {
	new_test_ext().execute_with(|| {
		let target: u64 = <Test as crate::Config>::TargetBlockTime::get();

		let t = bootstrap(1_000_000);
		let initial = CurrentDifficulty::<Test>::get();

		run_to_block_at(2, t + target);
		let after = CurrentDifficulty::<Test>::get();

		assert!(after == initial, "drift too large: {initial:?} -> {after:?}");
	});
}

#[test]
fn slow_block_decrease_difficulty() {
	new_test_ext().execute_with(|| {
		let target: u64 = <Test as crate::Config>::TargetBlockTime::get();

		let t = bootstrap(1_000_000);
		let before = CurrentDifficulty::<Test>::get();

		run_to_block_at(2, t + 2 * target);
		let after = CurrentDifficulty::<Test>::get();

		assert!(after < before, "slow block must lower difficulty: {before:?} -> {after:?}");
	});
}

#[test]
fn fast_block_increase_difficulty() {
	new_test_ext().execute_with(|| {
		let target: u64 = <Test as crate::Config>::TargetBlockTime::get();

		let t = bootstrap(1_000_000);
		let before = CurrentDifficulty::<Test>::get();

		run_to_block_at(2, t + target / 2);
		let after = CurrentDifficulty::<Test>::get();

		assert!(before < after, "fast block must higher difficulty: {before:?} -> {after:?}");
	});
}

#[test]
fn anchor_unchanged_during_normal_operation() {
	new_test_ext().execute_with(|| {
		let target: u64 = <Test as crate::Config>::TargetBlockTime::get();
		
		let mut t = bootstrap(1_000_000);
		let anchor_h = AnchorHeight::<Test>::get();
		let anchor_ts = AnchorTimestamp::<Test>::get();

		for i in 2u64..=6 {
			t = run_to_block_at(i, t + target);
		}

		assert_eq!(AnchorHeight::<Test>::get(), anchor_h, "anchor height must not move");
		assert_eq!(AnchorTimestamp::<Test>::get(), anchor_ts, "anchor timestamp must not move");
	});
}

#[test]
fn anchor_unchanged_when_gap_below_threshold() {
	new_test_ext().execute_with(|| {
		let target: u64 = <Test as crate::Config>::TargetBlockTime::get();
		let break_threshold: u64 = <Test as crate::Config>::BreakThresholdSecs::get();

		let mut t = bootstrap(1_000_000);
		let anchor_h = AnchorHeight::<Test>::get();
		let anchor_ts = AnchorTimestamp::<Test>::get();

		t = run_to_block_at(2, t + target);
		t = run_to_block_at(3, t + target);
		t = run_to_block_at(4, t + break_threshold);

		assert_eq!(AnchorHeight::<Test>::get(), anchor_h, "anchor height must not move");
		assert_eq!(AnchorTimestamp::<Test>::get(), anchor_ts, "anchor timestamp must not move");
		assert_eq!(LastBlockTimestamp::<Test>::get(), t);
	});
}

#[test]
fn anchor_changed_when_gap_exceeds_threshold() {
	new_test_ext().execute_with(|| {
		let target: u64 = <Test as crate::Config>::TargetBlockTime::get();
		let break_threshold: u64 = <Test as crate::Config>::BreakThresholdSecs::get();

		let mut t = bootstrap(1_000_000);

		t = run_to_block_at(2, t + target);
		t = run_to_block_at(3, t + target);
		t = run_to_block_at(4, t + break_threshold + 1);

		assert_eq!(AnchorHeight::<Test>::get(), 4, "anchor must move to recovery block");
		assert_eq!(AnchorTimestamp::<Test>::get(), t);
		assert_eq!(LastBlockTimestamp::<Test>::get(), t);
	});
}

#[test]
fn realtime_difficulty_halves_after_halflife() {
	new_test_ext().execute_with(|| {
		let target: u64 = <Test as crate::Config>::TargetBlockTime::get();
		let halflife: u64 = <Test as crate::Config>::Halflife::get();

		let t = bootstrap(1_000_000);
		let initial = CurrentDifficulty::<Test>::get();

		let realtime_difficulty = crate::Pallet::<Test>::realtime_difficulty(t + halflife + target);
		let realtime_difficulty_2 = realtime_difficulty + realtime_difficulty;

		assert!(realtime_difficulty_2 == initial, "drift too large: {initial:?} -> {realtime_difficulty:?}");
	});
}
