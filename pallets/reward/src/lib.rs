//! Block reward pallet.
//!
//! Reads the PoW pre-runtime digest to identify the block author (miner),
//! then mints a configurable reward to their account on each block.
//!
//! Orphan and uncle blocks receive no reward because their state changes
//! are never applied to the canonical chain.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use codec::Decode;
	use frame_support::{
		pallet_prelude::*,
		traits::Currency,
	};
	use frame_system::pallet_prelude::*;
	use sp_consensus_pow::POW_ENGINE_ID;

	type BalanceOf<T> =
		<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The currency used to mint block rewards.
		type Currency: Currency<Self::AccountId>;

		/// Fixed reward per block (in smallest units).
		#[pallet::constant]
		type BlockReward: Get<BalanceOf<Self>>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_finalize(_n: BlockNumberFor<T>) {
			if let Some(author) = Self::find_author() {
				let reward = T::BlockReward::get();
				if !reward.is_zero() {
					let _ = T::Currency::deposit_creating(&author, reward);
				}
			}
		}
	}

	impl<T: Config> Pallet<T> {
		/// Extract the block author from the PoW pre-runtime digest.
		///
		/// The miner encodes their `AccountId` as the payload of a
		/// `PreRuntime(POW_ENGINE_ID, _)` digest item.
		fn find_author() -> Option<T::AccountId> {
			let digest = frame_system::Pallet::<T>::digest();
			for log in digest.logs.iter() {
				if let sp_runtime::DigestItem::PreRuntime(engine, data) = log {
					if *engine == POW_ENGINE_ID {
						return T::AccountId::decode(&mut &data[..]).ok();
					}
				}
			}
			None
		}
	}
}

#[cfg(test)]
mod tests {
	use codec::Encode;
	use frame_support::{
		derive_impl, parameter_types,
		traits::{ConstU128, Hooks},
	};
	use sp_consensus_pow::POW_ENGINE_ID;
	use sp_keyring::Sr25519Keyring;
	use sp_runtime::{AccountId32, BuildStorage, DigestItem, traits::IdentityLookup};

	type Balance = u128;

	#[frame_support::runtime]
	mod test_runtime {
		#[runtime::runtime]
		#[runtime::derive(
			RuntimeCall,
			RuntimeEvent,
			RuntimeError,
			RuntimeOrigin,
			RuntimeFreezeReason,
			RuntimeHoldReason,
			RuntimeSlashReason,
			RuntimeLockId,
			RuntimeTask
		)]
		pub struct Test;

		#[runtime::pallet_index(0)]
		pub type System = frame_system;

		#[runtime::pallet_index(1)]
		pub type Balances = pallet_balances;

		#[runtime::pallet_index(2)]
		pub type BlockReward = crate::pallet;
	}

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Test {
		type Block = frame_system::mocking::MockBlock<Test>;
		type AccountId = AccountId32;
		type Lookup = IdentityLookup<AccountId32>;
		type AccountData = pallet_balances::AccountData<Balance>;
	}

	#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
	impl pallet_balances::Config for Test {
		type AccountStore = System;
		type Balance = Balance;
		type ExistentialDeposit = ConstU128<1>;
	}

	parameter_types! {
		pub const Reward: Balance = 50_000_000_000_000_000_000; // 50 UNIT (18 decimals)
	}

	impl crate::pallet::Config for Test {
		type Currency = Balances;
		type BlockReward = Reward;
	}

	fn new_test_ext() -> sp_io::TestExternalities {
		let t = frame_system::GenesisConfig::<Test>::default()
			.build_storage()
			.unwrap();
		t.into()
	}

	fn set_author_digest(author: &sp_runtime::AccountId32) {
		let digest_item = DigestItem::PreRuntime(POW_ENGINE_ID, author.encode());
		frame_system::Pallet::<Test>::deposit_log(digest_item);
	}

	#[test]
	fn mints_reward_to_block_author() {
		new_test_ext().execute_with(|| {
			let miner = Sr25519Keyring::Alice.to_account_id();
			set_author_digest(&miner);

			crate::pallet::Pallet::<Test>::on_finalize(1);

			assert_eq!(
				pallet_balances::Pallet::<Test>::free_balance(miner),
				50_000_000_000_000_000_000u128, // 50 UNIT
			);
		});
	}

	#[test]
	fn no_reward_without_digest() {
		new_test_ext().execute_with(|| {
			let miner = Sr25519Keyring::Alice.to_account_id();
			// No digest set
			crate::pallet::Pallet::<Test>::on_finalize(1);

			assert_eq!(
				pallet_balances::Pallet::<Test>::free_balance(miner),
				0,
			);
		});
	}

	#[test]
	fn ignores_non_pow_digest() {
		new_test_ext().execute_with(|| {
			let miner = Sr25519Keyring::Alice.to_account_id();
			// Use a different engine ID
			let digest_item = DigestItem::PreRuntime(*b"aura", miner.encode());
			frame_system::Pallet::<Test>::deposit_log(digest_item);

			crate::pallet::Pallet::<Test>::on_finalize(1);

			assert_eq!(
				pallet_balances::Pallet::<Test>::free_balance(miner),
				0,
			);
		});
	}

	#[test]
	fn reward_accumulates_over_blocks() {
		new_test_ext().execute_with(|| {
			let miner = Sr25519Keyring::Alice.to_account_id();

			// Block 1
			set_author_digest(&miner);
			crate::pallet::Pallet::<Test>::on_finalize(1);

			// Block 2
			set_author_digest(&miner);
			crate::pallet::Pallet::<Test>::on_finalize(2);

			assert_eq!(
				pallet_balances::Pallet::<Test>::free_balance(miner),
				100_000_000_000_000_000_000u128, // 100 UNIT
			);
		});
	}
}
