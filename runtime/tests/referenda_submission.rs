//! Referendum submission gate. Only accounts backed by an identity judged
//! Reasonable or KnownGood may open referenda, and a sub account qualifies
//! through its parent judgement.

mod common;

use codec::Encode;
use common::new_test_ext;
use frame_support::{
	assert_noop, assert_ok,
	traits::{schedule::DispatchTime, tokens::fungible::Mutate, Bounded},
};
use numen_runtime::{
	configs::governance::pallet_custom_origins, AccountId, Balance, Balances, Identity, Referenda,
	Runtime, RuntimeCall, RuntimeOrigin, UNIT,
};
use pallet_identity::{legacy::IdentityInfo, Data, Judgement};
use sp_keyring::Sr25519Keyring;
use sp_runtime::{
	traits::{Hash, StaticLookup},
	DispatchError, DispatchResult,
};

const FUNDS: Balance = 10_000 * UNIT;

type IdInfo = <Runtime as pallet_identity::Config>::IdentityInformation;

fn src(who: &AccountId) -> <<Runtime as frame_system::Config>::Lookup as StaticLookup>::Source {
	<Runtime as frame_system::Config>::Lookup::unlookup(who.clone())
}

fn funded(keyring: Sr25519Keyring) -> AccountId {
	let who = keyring.to_account_id();
	Balances::set_balance(&who, FUNDS);
	who
}

fn identity_info() -> IdInfo {
	IdentityInfo {
		additional: Default::default(),
		display: Data::Raw(b"proposer".to_vec().try_into().unwrap()),
		legal: Default::default(),
		web: Default::default(),
		riot: Default::default(),
		email: Default::default(),
		pgp_fingerprint: None,
		image: Default::default(),
		twitter: Default::default(),
	}
}

/// Installs Eve as registrar zero through the prime key.
fn install_registrar() -> AccountId {
	let prime = Sr25519Keyring::Ferdie.to_account_id();
	pallet_prime::Key::<Runtime>::put(&prime);
	let registrar = funded(Sr25519Keyring::Eve);
	assert_ok!(Identity::add_registrar(
		RuntimeOrigin::signed(prime),
		src(&registrar),
	));
	registrar
}

/// Gives `who` an identity carrying `judgement` from registrar zero.
fn judged_identity(who: &AccountId, judgement: Judgement<Balance>) {
	let registrar = install_registrar();
	assert_ok!(Identity::set_identity(
		RuntimeOrigin::signed(who.clone()),
		Box::new(identity_info()),
	));
	assert_ok!(Identity::provide_judgement(
		RuntimeOrigin::signed(registrar),
		0,
		src(who),
		judgement,
		<Runtime as frame_system::Config>::Hashing::hash_of(&identity_info()),
	));
}

fn submit(who: &AccountId) -> DispatchResult {
	let call = RuntimeCall::System(frame_system::Call::remark { remark: vec![] });
	Referenda::submit(
		RuntimeOrigin::signed(who.clone()),
		Box::new(pallet_custom_origins::Origin::SmallSpender.into()),
		Bounded::Inline(call.encode().try_into().expect("remark fits the inline bound")),
		DispatchTime::After(0),
	)
}

fn referendum_count() -> u32 {
	pallet_referenda::ReferendumCount::<Runtime>::get()
}

#[test]
fn plain_account_cannot_submit() {
	new_test_ext().execute_with(|| {
		let who = funded(Sr25519Keyring::Alice);

		assert_noop!(submit(&who), DispatchError::BadOrigin);
		assert_eq!(referendum_count(), 0);
	});
}

#[test]
fn unjudged_identity_cannot_submit() {
	new_test_ext().execute_with(|| {
		let who = funded(Sr25519Keyring::Alice);
		assert_ok!(Identity::set_identity(
			RuntimeOrigin::signed(who.clone()),
			Box::new(identity_info()),
		));

		assert_noop!(submit(&who), DispatchError::BadOrigin);
		assert_eq!(referendum_count(), 0);
	});
}

#[test]
fn negative_judgement_cannot_submit() {
	new_test_ext().execute_with(|| {
		let who = funded(Sr25519Keyring::Alice);
		judged_identity(&who, Judgement::OutOfDate);

		assert_noop!(submit(&who), DispatchError::BadOrigin);
		assert_eq!(referendum_count(), 0);
	});
}

#[test]
fn reasonable_judgement_submits() {
	new_test_ext().execute_with(|| {
		let who = funded(Sr25519Keyring::Alice);
		judged_identity(&who, Judgement::Reasonable);

		assert_ok!(submit(&who));
		assert_eq!(referendum_count(), 1);
	});
}

#[test]
fn known_good_judgement_submits() {
	new_test_ext().execute_with(|| {
		let who = funded(Sr25519Keyring::Alice);
		judged_identity(&who, Judgement::KnownGood);

		assert_ok!(submit(&who));
		assert_eq!(referendum_count(), 1);
	});
}

#[test]
fn sub_of_judged_identity_submits() {
	new_test_ext().execute_with(|| {
		let parent = funded(Sr25519Keyring::Alice);
		judged_identity(&parent, Judgement::Reasonable);
		let sub = funded(Sr25519Keyring::Bob);
		assert_ok!(Identity::set_subs(
			RuntimeOrigin::signed(parent),
			vec![(sub.clone(), Data::None)],
		));

		assert_ok!(submit(&sub));
		assert_eq!(referendum_count(), 1);
	});
}

#[test]
fn sub_of_unjudged_identity_cannot_submit() {
	new_test_ext().execute_with(|| {
		let parent = funded(Sr25519Keyring::Alice);
		assert_ok!(Identity::set_identity(
			RuntimeOrigin::signed(parent.clone()),
			Box::new(identity_info()),
		));
		let sub = funded(Sr25519Keyring::Bob);
		assert_ok!(Identity::set_subs(
			RuntimeOrigin::signed(parent),
			vec![(sub.clone(), Data::None)],
		));

		assert_noop!(submit(&sub), DispatchError::BadOrigin);
		assert_eq!(referendum_count(), 0);
	});
}
