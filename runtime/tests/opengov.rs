//! OpenGov wiring. The spender track origin approves treasury backed bounties
//! while runtime level power stays on the root track.

mod common;

use common::new_test_ext;
use frame_support::{assert_noop, assert_ok, traits::tokens::fungible::Mutate};
use pallet_bounties::BountyStatus;
use solochain_template_runtime::{
	configs::governance::pallet_custom_origins, Balances, Bounties, Runtime, RuntimeOrigin, UNIT,
};
use sp_keyring::Sr25519Keyring;
use sp_runtime::DispatchError;

/// Fund a proposer and place a bounty, returning its id.
fn proposed_bounty() -> u32 {
	let proposer = Sr25519Keyring::Alice.to_account_id();
	Balances::set_balance(&proposer, 10_000 * UNIT);
	let id = pallet_bounties::BountyCount::<Runtime>::get();
	assert_ok!(Bounties::propose_bounty(
		RuntimeOrigin::signed(proposer),
		1_000 * UNIT,
		b"work".to_vec(),
	));
	id
}

fn assert_queued_for_funding(id: u32) {
	let bounty = pallet_bounties::Bounties::<Runtime>::get(id).expect("bounty exists");
	assert!(matches!(bounty.get_status(), BountyStatus::Approved));
	assert!(
		pallet_bounties::BountyApprovals::<Runtime>::get().contains(&id),
		"an approved bounty must be queued for the next spend period",
	);
}

#[test]
fn spender_origin_approves_bounty() {
	new_test_ext().execute_with(|| {
		let id = proposed_bounty();
		let spender = RuntimeOrigin::from(pallet_custom_origins::Origin::Spender);

		assert_ok!(Bounties::approve_bounty(spender, id));

		assert_queued_for_funding(id);
	});
}

#[test]
fn root_origin_still_approves_bounty() {
	new_test_ext().execute_with(|| {
		let id = proposed_bounty();

		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), id));

		assert_queued_for_funding(id);
	});
}

#[test]
fn signed_origin_cannot_approve_bounty() {
	new_test_ext().execute_with(|| {
		let id = proposed_bounty();
		let who = Sr25519Keyring::Bob.to_account_id();

		assert_noop!(
			Bounties::approve_bounty(RuntimeOrigin::signed(who), id),
			DispatchError::BadOrigin,
		);

		let bounty = pallet_bounties::Bounties::<Runtime>::get(id).expect("bounty exists");
		assert!(
			matches!(bounty.get_status(), BountyStatus::Proposed),
			"a rejected approval must leave the bounty proposed",
		);
		assert!(pallet_bounties::BountyApprovals::<Runtime>::get().is_empty());
	});
}
