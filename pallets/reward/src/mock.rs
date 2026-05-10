use crate as pallet_reward;
use codec::Encode;
use frame_support::{derive_impl, traits::{ConstU128, Hooks}};
use sp_consensus_pow::POW_ENGINE_ID;
use sp_runtime::{AccountId32, BuildStorage, DigestItem, traits::IdentityLookup};

pub type Balance = u128;

#[frame_support::runtime]
mod runtime {
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
	pub type System = frame_system::Pallet<Test>;

	#[runtime::pallet_index(1)]
	pub type Balances = pallet_balances::Pallet<Test>;

	#[runtime::pallet_index(2)]
	pub type BlockReward = pallet_reward::Pallet<Test>;
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

impl pallet_reward::Config for Test {
	type Currency = Balances;
	type BlockReward = ConstU128<1>;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let storage = frame_system::GenesisConfig::<Test>::default()
		.build_storage()
		.unwrap();
	storage.into()
}

pub fn run_to_block_at(block: u64, author: &AccountId32)  {
	let digest_item = DigestItem::PreRuntime(POW_ENGINE_ID, author.encode());
	frame_system::Pallet::<Test>::deposit_log(digest_item);
	pallet_reward::Pallet::<Test>::on_finalize(block);
}
