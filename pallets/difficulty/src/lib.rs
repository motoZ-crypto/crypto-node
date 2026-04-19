//! ASERT difficulty adjustment pallet.
//!
//! Stores the current mining difficulty and anchor block parameters used by
//! the ASERT algorithm. The actual difficulty calculation logic (ASERT
//! formula) is added in a follow-up issue; this pallet provides the
//! scaffolding, storage, genesis configuration, and a public query method
//! that the `DifficultyApi` runtime API delegates to.

#![cfg_attr(not(feature = "std"), no_std)]

pub mod asert;
pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::pallet_prelude::*;
	use sp_core::U256;

	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event>> + pallet_timestamp::Config {
		/// Target block time in seconds (e.g. 20).
		#[pallet::constant]
		type TargetBlockTime: Get<u64>;

		/// ASERT halflife in seconds (e.g. 1800 = 30 minutes).
		#[pallet::constant]
		type Halflife: Get<u64>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	// ── Storage ─────────────────────────────────────────────────────

	/// Current mining difficulty (U256).
	///
	/// Updated each block once ASERT calculation is wired in (#023b).
	/// Initially set via genesis config.
	#[pallet::storage]
	#[pallet::getter(fn current_difficulty)]
	pub type CurrentDifficulty<T: Config> = StorageValue<_, U256, ValueQuery>;

	/// Anchor block target value (inverse of difficulty).
	///
	/// `target = U256::MAX / difficulty`. Set at genesis.
	#[pallet::storage]
	#[pallet::getter(fn anchor_target)]
	pub type AnchorTarget<T: Config> = StorageValue<_, U256, ValueQuery>;

	/// Timestamp of the anchor block's parent (seconds since Unix epoch).
	#[pallet::storage]
	#[pallet::getter(fn anchor_timestamp)]
	pub type AnchorTimestamp<T: Config> = StorageValue<_, u64, ValueQuery>;

	/// Block height of the anchor block.
	#[pallet::storage]
	#[pallet::getter(fn anchor_height)]
	pub type AnchorHeight<T: Config> = StorageValue<_, u32, ValueQuery>;

	// ── Events ──────────────────────────────────────────────────────

	#[pallet::event]
	pub enum Event {
		/// Difficulty was adjusted to a new value.
		DifficultyAdjusted {
			/// The new difficulty.
			difficulty: U256,
		},
	}

	// ── Errors ──────────────────────────────────────────────────────

	#[pallet::error]
	pub enum Error<T> {
		/// The difficulty value overflowed or underflowed during calculation.
		DifficultyOverflow,
		/// Zero difficulty is not allowed.
		ZeroDifficulty,
	}

	// ── Genesis ─────────────────────────────────────────────────────

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		/// Initial mining difficulty.
		pub initial_difficulty: U256,
		/// Anchor target (U256::MAX / initial_difficulty). If zero, it is
		/// computed automatically from `initial_difficulty`.
		pub anchor_target: U256,
		/// Anchor timestamp (seconds). Typically 0 for genesis.
		pub anchor_timestamp: u64,
		/// Anchor block height. Typically 0 for genesis.
		pub anchor_height: u32,
		#[serde(skip)]
		pub _marker: core::marker::PhantomData<T>,
	}

	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self {
				initial_difficulty: Default::default(),
				anchor_target: Default::default(),
				anchor_timestamp: Default::default(),
				anchor_height: Default::default(),
				_marker: Default::default(),
			}
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			CurrentDifficulty::<T>::put(self.initial_difficulty);

			// Compute anchor target if not explicitly set.
			let target = if self.anchor_target == U256::zero() && self.initial_difficulty != U256::zero() {
				U256::MAX / self.initial_difficulty
			} else {
				self.anchor_target
			};
			AnchorTarget::<T>::put(target);
			AnchorTimestamp::<T>::put(self.anchor_timestamp);
			AnchorHeight::<T>::put(self.anchor_height);
		}
	}
}
