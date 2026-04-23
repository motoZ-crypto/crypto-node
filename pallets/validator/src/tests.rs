use crate::{
	mock::*, Error, Event, LockInfo, PendingValidators, RejoinCooldown, ValidatorLocks,
	ValidatorStatus,
};
use frame_support::{assert_noop, assert_ok, traits::Hooks};
use sp_runtime::{traits::Dispatchable, DispatchError, TokenError};

/// Advance the chain to `target` block, calling `on_initialize` for each new block.
fn run_to_block(target: u64) {
	while System::block_number() < target {
		let next = System::block_number() + 1;
		System::set_block_number(next);
		Validator::on_initialize(next);
	}
}

#[test]
fn lock_succeeds_and_records_state() {
	new_test_ext().execute_with(|| {
		assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));

		let lock = ValidatorLocks::<Test>::get(ALICE).expect("lock recorded");
		assert_eq!(
			lock,
			LockInfo {
				amount: 1_000,
				lock_block: 1,
				expiry_block: 11,
				status: ValidatorStatus::Active,
			}
		);
		assert_eq!(PendingValidators::<Test>::get().to_vec(), vec![ALICE]);
		System::assert_last_event(
			Event::ValidatorLocked { who: ALICE, amount: 1_000, expiry_block: 11 }.into(),
		);
	});
}

#[test]
fn lock_fails_when_balance_insufficient() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Validator::lock(RuntimeOrigin::signed(EVE)),
			Error::<Test>::InsufficientBalance
		);
		assert!(ValidatorLocks::<Test>::get(EVE).is_none());
		assert!(PendingValidators::<Test>::get().is_empty());
	});
}

#[test]
fn lock_fails_when_already_validator() {
	new_test_ext().execute_with(|| {
		assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
		assert_noop!(
			Validator::lock(RuntimeOrigin::signed(ALICE)),
			Error::<Test>::AlreadyValidator
		);
	});
}

#[test]
fn lock_rejected_during_active_cooldown() {
	new_test_ext().execute_with(|| {
		RejoinCooldown::<Test>::insert(ALICE, 100u64);
		assert_noop!(
			Validator::lock(RuntimeOrigin::signed(ALICE)),
			Error::<Test>::InCooldown
		);
		assert!(RejoinCooldown::<Test>::get(ALICE).is_some());
	});
}

#[test]
fn lock_succeeds_after_cooldown_expires_and_clears_record() {
	new_test_ext().execute_with(|| {
		RejoinCooldown::<Test>::insert(ALICE, 1u64);
		System::set_block_number(2);
		assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
		assert!(RejoinCooldown::<Test>::get(ALICE).is_none());
	});
}

#[test]
fn locked_balance_cannot_be_transferred() {
	new_test_ext().execute_with(|| {
		assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));

		// Free balance is 10_000 and 1_000 is locked. Anything that would push the
		// usable balance below 1_000 must be rejected by pallet-balances.
		let call = pallet_balances::Call::<Test>::transfer_keep_alive {
			dest: BOB,
			value: 9_500,
		};
		let res = RuntimeCall::Balances(call).dispatch(RuntimeOrigin::signed(ALICE));
		assert_eq!(res.unwrap_err().error, DispatchError::Token(TokenError::Frozen));

		// A transfer that respects the lock still works.
		assert_ok!(Balances::transfer_keep_alive(RuntimeOrigin::signed(ALICE), BOB, 8_000));
	});
}

#[test]
fn lock_fails_when_pending_queue_full() {
	new_test_ext().execute_with(|| {
		assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
		assert_ok!(Validator::lock(RuntimeOrigin::signed(BOB)));
		assert_ok!(Validator::lock(RuntimeOrigin::signed(CHARLIE)));
		assert_noop!(
			Validator::lock(RuntimeOrigin::signed(DAVE)),
			Error::<Test>::TooManyValidators
		);
		assert!(ValidatorLocks::<Test>::get(DAVE).is_none());
	});
}

#[test]
fn auto_renew_skips_within_interval() {
	new_test_ext().execute_with(|| {
		// Lock at block 1 -> expiry = 11.
		assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
		// At block 5: expiry - now = 6, elapsed_window = 5, not > 5 -> no renewal.
		run_to_block(5);
		let lock = ValidatorLocks::<Test>::get(ALICE).expect("lock recorded");
		assert_eq!(lock.expiry_block, 11);
	});
}

#[test]
fn auto_renew_extends_active_validator_lock() {
	new_test_ext().execute_with(|| {
		assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
		// At block 6: expiry - now = 5, elapsed_window = 5 >= 5 -> renew.
		// New expiry = 6 + 10 = 16.
		run_to_block(6);
		let lock = ValidatorLocks::<Test>::get(ALICE).expect("lock recorded");
		assert_eq!(lock.expiry_block, 16);
		assert_eq!(lock.lock_block, 1);
		assert_eq!(lock.status, ValidatorStatus::Active);
		System::assert_last_event(
			Event::ValidatorLocked { who: ALICE, amount: 1_000, expiry_block: 16 }.into(),
		);
	});
}

#[test]
fn auto_renew_skips_non_active_status() {
	new_test_ext().execute_with(|| {
		assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
		// Mark the validator as having requested exit; renewal must stop.
		ValidatorLocks::<Test>::mutate(ALICE, |maybe| {
			maybe.as_mut().unwrap().status = ValidatorStatus::ExitRequested;
		});
		run_to_block(8);
		let lock = ValidatorLocks::<Test>::get(ALICE).expect("lock recorded");
		assert_eq!(lock.expiry_block, 11);
		assert_eq!(lock.status, ValidatorStatus::ExitRequested);
	});
}
