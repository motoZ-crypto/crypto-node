use crate::{
    mock::*, ActiveValidators, Error, Event, KickReason, LockInfo, OfflineSessionCount,
    OfflineThisSession, PendingValidators, RejoinCooldown, ValidatorLocks, ValidatorStatus,
};
use frame_support::{assert_noop, assert_ok, traits::Hooks};
use pallet_session::SessionManager;
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

#[test]
fn request_exit_changes_status_and_removes_from_pending() {
    new_test_ext().execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Validator::lock(RuntimeOrigin::signed(BOB)));
        assert_eq!(PendingValidators::<Test>::get().to_vec(), vec![ALICE, BOB]);

        assert_ok!(Validator::request_exit(RuntimeOrigin::signed(ALICE)));

        let lock = ValidatorLocks::<Test>::get(ALICE).expect("lock kept");
        assert_eq!(lock.status, ValidatorStatus::ExitRequested);
        assert_eq!(lock.expiry_block, 11);
        assert_eq!(PendingValidators::<Test>::get().to_vec(), vec![BOB]);
        System::assert_last_event(Event::ValidatorExitRequested { who: ALICE }.into());
    });
}

#[test]
fn request_exit_fails_when_not_validator() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            Validator::request_exit(RuntimeOrigin::signed(ALICE)),
            Error::<Test>::NotValidator
        );
    });
}

#[test]
fn request_exit_fails_when_not_active() {
    new_test_ext().execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Validator::request_exit(RuntimeOrigin::signed(ALICE)));
        // Second call: status is ExitRequested, not Active.
        assert_noop!(
            Validator::request_exit(RuntimeOrigin::signed(ALICE)),
            Error::<Test>::InvalidStatus
        );

        // Kicked status also rejected.
        ValidatorLocks::<Test>::mutate(BOB, |maybe| {
            *maybe = Some(LockInfo {
                amount: 1_000,
                lock_block: 1,
                expiry_block: 11,
                status: ValidatorStatus::Kicked,
            });
        });
        assert_noop!(
            Validator::request_exit(RuntimeOrigin::signed(BOB)),
            Error::<Test>::InvalidStatus
        );
    });
}

#[test]
fn request_exit_stops_auto_renewal() {
    new_test_ext().execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Validator::request_exit(RuntimeOrigin::signed(ALICE)));
        // Pass the renewal threshold: without exit, expiry would extend at block 6.
        run_to_block(8);
        let lock = ValidatorLocks::<Test>::get(ALICE).expect("lock kept");
        assert_eq!(lock.expiry_block, 11);
        assert_eq!(lock.status, ValidatorStatus::ExitRequested);
    });
}

#[test]
fn request_exit_keeps_lock_until_expiry() {
    new_test_ext().execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Validator::request_exit(RuntimeOrigin::signed(ALICE)));

        // Lock is still enforced: cannot move balance below the locked amount.
        let call = pallet_balances::Call::<Test>::transfer_keep_alive {
            dest: BOB,
            value: 9_500,
        };
        let res = RuntimeCall::Balances(call).dispatch(RuntimeOrigin::signed(ALICE));
		assert_eq!(res.unwrap_err().error, DispatchError::Token(TokenError::Frozen));
    });
}

#[test]
fn lock_released_when_expiry_reached() {
    new_test_ext().execute_with(|| {
        // Lock at block 1 -> expiry = 11. Request exit so renewal does not extend it.
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Validator::request_exit(RuntimeOrigin::signed(ALICE)));

        // Right before expiry: lock still in place.
        run_to_block(10);
        assert!(ValidatorLocks::<Test>::get(ALICE).is_some());

        // At expiry block: lock released, storage cleared, event emitted.
        run_to_block(11);
        assert!(ValidatorLocks::<Test>::get(ALICE).is_none());
        System::assert_last_event(
			Event::LockReleased { who: ALICE, amount: 1_000 }.into(),
        );

        // Funds are fully transferable again.
        assert_ok!(Balances::transfer_keep_alive(
            RuntimeOrigin::signed(ALICE),
            BOB,
            9_500
        ));
    });
}

#[test]
fn unexpired_locks_not_released() {
    new_test_ext().execute_with(|| {
        // Two validators locked at block 1; both expire at block 11.
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Validator::lock(RuntimeOrigin::signed(BOB)));
        // Only ALICE requests exit so BOB keeps renewing.
        assert_ok!(Validator::request_exit(RuntimeOrigin::signed(ALICE)));

        // Advance to block 11: ALICE expires and is released. BOB renews twice
        // (at blocks 6 and 11), so its expiry advances to 21 and stays Active.
        run_to_block(11);
        assert!(ValidatorLocks::<Test>::get(ALICE).is_none());
        let bob = ValidatorLocks::<Test>::get(BOB).expect("bob lock kept");
        assert_eq!(bob.expiry_block, 21);
        assert_eq!(bob.status, ValidatorStatus::Active);
    });
}

#[test]
fn released_account_can_relock() {
    new_test_ext().execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Validator::request_exit(RuntimeOrigin::signed(ALICE)));
        run_to_block(11);
        assert!(ValidatorLocks::<Test>::get(ALICE).is_none());

        // Storage is cleaned up, so a fresh lock call must succeed.
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        let lock = ValidatorLocks::<Test>::get(ALICE).expect("relock recorded");
        assert_eq!(lock.lock_block, 11);
        assert_eq!(lock.expiry_block, 21);
        assert_eq!(lock.status, ValidatorStatus::Active);
    });
}

#[test]
fn new_session_promotes_pending_validators() {
    new_test_ext().execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Validator::lock(RuntimeOrigin::signed(BOB)));

        let set = <Validator as SessionManager<AccountId>>::new_session(1)
            .expect("set must change from empty");
        assert_eq!(set, vec![ALICE, BOB]);
        assert_eq!(ActiveValidators::<Test>::get().to_vec(), vec![ALICE, BOB]);
        assert!(PendingValidators::<Test>::get().is_empty());
    });
}

#[test]
fn new_session_removes_exited_validator() {
    new_test_ext().execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Validator::lock(RuntimeOrigin::signed(BOB)));
        let _ = <Validator as SessionManager<AccountId>>::new_session(1);

        assert_ok!(Validator::request_exit(RuntimeOrigin::signed(ALICE)));

        let set = <Validator as SessionManager<AccountId>>::new_session(2)
            .expect("set must change after exit");
        assert_eq!(set, vec![BOB]);
        assert_eq!(ActiveValidators::<Test>::get().to_vec(), vec![BOB]);
    });
}

#[test]
fn new_session_removes_kicked_and_cooldown_validators() {
    new_test_ext().execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Validator::lock(RuntimeOrigin::signed(BOB)));
        assert_ok!(Validator::lock(RuntimeOrigin::signed(CHARLIE)));
        let _ = <Validator as SessionManager<AccountId>>::new_session(1);

        // Simulate kick / cooldown by mutating storage directly.
        ValidatorLocks::<Test>::mutate(BOB, |maybe| {
            maybe.as_mut().unwrap().status = ValidatorStatus::Kicked;
        });
        ValidatorLocks::<Test>::mutate(CHARLIE, |maybe| {
            maybe.as_mut().unwrap().status = ValidatorStatus::Cooldown;
        });

        let set = <Validator as SessionManager<AccountId>>::new_session(2)
            .expect("set must change after kick/cooldown");
        assert_eq!(set, vec![ALICE]);
        assert_eq!(ActiveValidators::<Test>::get().to_vec(), vec![ALICE]);
    });
}

#[test]
fn new_session_returns_none_when_unchanged() {
    new_test_ext().execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        let _ = <Validator as SessionManager<AccountId>>::new_session(1);
        // No new lock and no exit: next session must be a no-op.
        assert!(<Validator as SessionManager<AccountId>>::new_session(2).is_none());
        assert_eq!(ActiveValidators::<Test>::get().to_vec(), vec![ALICE]);
    });
}

#[test]
fn new_session_drops_validator_with_released_lock() {
    new_test_ext().execute_with(|| {
        // Lock and promote ALICE to active.
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        let _ = <Validator as SessionManager<AccountId>>::new_session(1);
        assert_eq!(ActiveValidators::<Test>::get().to_vec(), vec![ALICE]);

        // Exit and let the lock expire so ValidatorLocks is cleared.
        assert_ok!(Validator::request_exit(RuntimeOrigin::signed(ALICE)));
        run_to_block(11);
        assert!(ValidatorLocks::<Test>::get(ALICE).is_none());

        let set = <Validator as SessionManager<AccountId>>::new_session(2)
            .expect("set must shrink to empty");
        assert!(set.is_empty());
        assert!(ActiveValidators::<Test>::get().is_empty());
    });
}

/// Helper: mark `who` as offline for the current session window.
fn report_offline(who: AccountId) {
    Validator::note_offline(&who);
}

/// Helper: advance to the next session boundary by invoking `new_session`.
fn rotate_session(index: u32) -> Option<alloc::vec::Vec<AccountId>> {
    <Validator as SessionManager<AccountId>>::new_session(index)
}

#[test]
fn note_offline_ignores_non_validator() {
    new_test_ext().execute_with(|| {
        Validator::note_offline(&ALICE);
        assert!(OfflineThisSession::<Test>::get(ALICE).is_none());
    });
}

#[test]
fn consecutive_offline_kicks_validator() {
    new_test_ext().execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Validator::lock(RuntimeOrigin::signed(BOB)));
        let _ = rotate_session(1);
        assert_eq!(ActiveValidators::<Test>::get().to_vec(), vec![ALICE, BOB]);

        // ALICE misses three consecutive sessions while BOB stays online.
        for idx in 2..=4u32 {
            report_offline(ALICE);
            let _ = rotate_session(idx);
        }

        let alice_lock = ValidatorLocks::<Test>::get(ALICE).expect("lock retained");
        assert_eq!(alice_lock.status, ValidatorStatus::Kicked);
        assert!(RejoinCooldown::<Test>::get(ALICE).is_some());
        assert_eq!(OfflineSessionCount::<Test>::get(ALICE), 0);
        assert_eq!(ActiveValidators::<Test>::get().to_vec(), vec![BOB]);
        // The transient set must be empty after processing.
        assert!(OfflineThisSession::<Test>::iter().next().is_none());
        // Event emitted with `Offline` reason.
        let kicked = System::events().into_iter().any(|e| matches!(
            e.event,
            RuntimeEvent::Validator(Event::ValidatorKicked { who: ALICE, reason: KickReason::Offline })
        ));
        assert!(kicked, "ValidatorKicked event with Offline reason expected");
    });
}

#[test]
fn intermittent_heartbeat_resets_counter() {
    new_test_ext().execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        let _ = rotate_session(1);

        // Two offline sessions: counter reaches 2, still below threshold (3).
        report_offline(ALICE);
        let _ = rotate_session(2);
        report_offline(ALICE);
        let _ = rotate_session(3);
        assert_eq!(OfflineSessionCount::<Test>::get(ALICE), 2);

        // Heartbeat received this session: counter resets.
        let _ = rotate_session(4);
        assert_eq!(OfflineSessionCount::<Test>::get(ALICE), 0);

        // Two more offline sessions: only counter == 2 again, still no kick.
        report_offline(ALICE);
        let _ = rotate_session(5);
        report_offline(ALICE);
        let _ = rotate_session(6);
        assert_eq!(OfflineSessionCount::<Test>::get(ALICE), 2);

        let lock = ValidatorLocks::<Test>::get(ALICE).expect("lock retained");
        assert_eq!(lock.status, ValidatorStatus::Active);
        assert!(RejoinCooldown::<Test>::get(ALICE).is_none());
    });
}

#[test]
fn kicked_validator_skips_auto_renewal() {
    new_test_ext().execute_with(|| {
        // Lock at block 1, expiry = 11. Renewal would extend at block 6.
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        let _ = rotate_session(1);

        // Drive ALICE offline for three sessions to trigger a kick.
        for idx in 2..=4u32 {
            report_offline(ALICE);
            let _ = rotate_session(idx);
        }
        let lock = ValidatorLocks::<Test>::get(ALICE).expect("lock retained");
        assert_eq!(lock.status, ValidatorStatus::Kicked);
        assert_eq!(lock.expiry_block, 11);

        // Past the next renewal window: status is no longer Active so no extension.
        run_to_block(8);
        let lock = ValidatorLocks::<Test>::get(ALICE).expect("lock retained");
        assert_eq!(lock.expiry_block, 11);
    });
}

#[test]
fn lock_blocked_during_cooldown_then_succeeds() {
    new_test_ext().execute_with(|| {
        // Lock and kick ALICE.
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        let _ = rotate_session(1);
        for idx in 2..=4u32 {
            report_offline(ALICE);
            let _ = rotate_session(idx);
        }

        // Let the original lock expire so the account is free to relock.
        run_to_block(11);
        assert!(ValidatorLocks::<Test>::get(ALICE).is_none());

        // Cooldown still active: lock() must reject.
        let cooldown_until = RejoinCooldown::<Test>::get(ALICE).expect("cooldown set");
        assert!(cooldown_until > System::block_number());
        assert_noop!(
            Validator::lock(RuntimeOrigin::signed(ALICE)),
            Error::<Test>::InCooldown,
        );

        // Move past cooldown deadline; lock() now succeeds and the record is cleared.
        System::set_block_number(cooldown_until.saturating_add(1));
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert!(RejoinCooldown::<Test>::get(ALICE).is_none());
        let lock = ValidatorLocks::<Test>::get(ALICE).expect("relock recorded");
        assert_eq!(lock.status, ValidatorStatus::Active);
    });
}
