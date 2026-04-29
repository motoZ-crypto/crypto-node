use crate as pallet_difficulty;
use frame_support::{derive_impl, traits::ConstU64};
use sp_core::U256;
use sp_runtime::BuildStorage;

type Block = frame_system::mocking::MockBlock<Test>;

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
		RuntimeTask,
		RuntimeViewFunction
	)]
	pub struct Test;

	#[runtime::pallet_index(0)]
	pub type System = frame_system::Pallet<Test>;

	#[runtime::pallet_index(1)]
	pub type Timestamp = pallet_timestamp::Pallet<Test>;

	#[runtime::pallet_index(2)]
	pub type Difficulty = pallet_difficulty::Pallet<Test>;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

#[derive_impl(pallet_timestamp::config_preludes::TestDefaultConfig)]
impl pallet_timestamp::Config for Test {}

frame_support::parameter_types! {
	pub const TargetBlockTime: u64 = 20;
	pub const Halflife: u64 = 1800;
}

impl pallet_difficulty::Config for Test {
	type TargetBlockTime = TargetBlockTime;
	type Halflife = Halflife;
	type BreakThresholdSecs = ConstU64<1800>;
}

/// Initial difficulty used by tests.
pub const INITIAL_DIFFICULTY: u128 = 1_000_000;

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

	let initial = U256::from(INITIAL_DIFFICULTY);
	pallet_difficulty::GenesisConfig::<Test> {
		initial_difficulty: initial,
		anchor_target: U256::MAX / initial,
		anchor_timestamp: 0,
		anchor_height: 0,
		_marker: Default::default(),
	}
	.assimilate_storage(&mut storage)
	.unwrap();

	storage.into()
}

/// Advance to the given block, set the timestamp (in seconds), and run
/// `on_finalize` for both timestamp and difficulty pallets.
pub fn run_to_block_at(block: u64, now_secs: u64) {
	use frame_support::traits::Hooks;
	System::set_block_number(block);
	let _ = pallet_timestamp::Pallet::<Test>::set(
		frame_system::RawOrigin::None.into(),
		now_secs * 1000,
	);
	<pallet_difficulty::Pallet<Test> as Hooks<u64>>::on_finalize(block);
	// Clear timestamp's per-block DidUpdate flag so the next block can
	// set it again.
	<pallet_timestamp::Pallet<Test> as Hooks<u64>>::on_finalize(block);
}
