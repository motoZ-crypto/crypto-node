use crate::mock::*;
use codec::Encode;
use frame_support::traits::{Hooks, Get};
use sp_keyring::Sr25519Keyring;
use sp_runtime::DigestItem;

// Reward issuance tests.

#[test]
fn mints_reward_to_block_author() {
	new_test_ext().execute_with(|| {
		let reward = <Test as crate::Config>::BlockReward::get();

		let miner = Sr25519Keyring::Alice.to_account_id();
		run_to_block_at(1, &miner);

		assert_eq!(
			pallet_balances::Pallet::<Test>::free_balance(miner),
			reward,
		);
	});
}

#[test]
fn does_not_mint_reward_to_non_block_author() {
	new_test_ext().execute_with(|| {

		let miner1 = Sr25519Keyring::Alice.to_account_id();
		let miner2 = Sr25519Keyring::Bob.to_account_id();

		run_to_block_at(1, &miner1);

		assert_eq!(
			pallet_balances::Pallet::<Test>::free_balance(miner2),
			0,
		);
	});
}

#[test]
fn reward_accumulates_over_blocks() {
	new_test_ext().execute_with(|| {
		let reward: Balance = <Test as crate::Config>::BlockReward::get();

		let miner = Sr25519Keyring::Alice.to_account_id();

		run_to_block_at(1, &miner);
		run_to_block_at(2, &miner);

		assert_eq!(
			pallet_balances::Pallet::<Test>::free_balance(miner),
			reward * 2,
		);
	});
}

// Digest handling tests.

#[test]
fn no_reward_without_digest() {
	new_test_ext().execute_with(|| {
		let miner = Sr25519Keyring::Alice.to_account_id();
		crate::pallet::Pallet::<Test>::on_finalize(1);
		assert_eq!(pallet_balances::Pallet::<Test>::free_balance(miner), 0);
	});
}

#[test]
fn ignores_non_pow_digest() {
	new_test_ext().execute_with(|| {
		let miner = Sr25519Keyring::Alice.to_account_id();
		let digest_item = DigestItem::PreRuntime(*b"aura", miner.encode());
		frame_system::Pallet::<Test>::deposit_log(digest_item);

		crate::pallet::Pallet::<Test>::on_finalize(1);

		assert_eq!(pallet_balances::Pallet::<Test>::free_balance(miner), 0);
	});
}