//! OpenGov wiring. The spender track origin approves treasury backed bounties
//! while runtime level power stays on the root track.

use frame_support::{assert_noop, assert_ok, traits::tokens::fungible::Mutate};
use solochain_template_runtime::{
	configs::governance::{pallet_custom_origins, TracksInfo},
	Balance, Balances, BlockNumber, Bounties, Runtime, RuntimeOrigin, System, UNIT,
};
use sp_io::TestExternalities;
use sp_keyring::Sr25519Keyring;
use sp_runtime::{BuildStorage, DispatchError};

fn new_test_ext() -> TestExternalities {
	let storage = frame_system::GenesisConfig::<Runtime>::default()
		.build_storage()
		.expect("system genesis builds");
	let mut ext = TestExternalities::from(storage);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

/// Fund a proposer and place a bounty, returning its id.
fn proposed_bounty() -> u32 {
	let proposer = Sr25519Keyring::Alice.to_account_id();
	Balances::set_balance(&proposer, 10_000 * UNIT);
	assert_ok!(Bounties::propose_bounty(
		RuntimeOrigin::signed(proposer),
		1_000 * UNIT,
		b"work".to_vec(),
	));
	0
}

#[test]
fn spender_origin_approves_bounty() {
	new_test_ext().execute_with(|| {
		let id = proposed_bounty();
		let spender = RuntimeOrigin::from(pallet_custom_origins::Origin::Spender);
		assert_ok!(Bounties::approve_bounty(spender, id));
	});
}

#[test]
fn root_origin_still_approves_bounty() {
	new_test_ext().execute_with(|| {
		let id = proposed_bounty();
		assert_ok!(Bounties::approve_bounty(RuntimeOrigin::root(), id));
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
	});
}

#[test]
fn governance_exposes_root_and_spender_tracks() {
	let tracks: Vec<_> =
		<TracksInfo as pallet_referenda::TracksInfo<Balance, BlockNumber>>::tracks().collect();
	assert_eq!(tracks.len(), 2);
	assert_eq!(tracks[0].id, 0);
	assert_eq!(tracks[1].id, 1);
}
