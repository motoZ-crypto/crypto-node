//! # Pallet Validator
//!
//! Manages the full lifecycle of validators: lock, auto-renewal, exit,
//! and kick. This crate currently provides the storage, event, and error
//! skeleton.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::traits::Currency;
use scale_info::TypeInfo;

/// Balance type alias derived from the configured `Currency`.
pub type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

/// Lifecycle state of a validator's stake.
#[derive(
	Clone, Copy, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
pub enum ValidatorStatus {
	/// Active in the validator set; eligible for auto-renewal.
	Active,
	/// Voluntary exit requested; auto-renewal stopped, awaiting expiry.
	ExitRequested,
	/// Kicked due to offline or equivocation; in cooldown.
	Kicked,
	/// Cooldown period after equivocation kick.
	Cooldown,
}

/// Lock record for a validator's stake.
#[derive(
	Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
)]
pub struct LockInfo<Balance, BlockNumber> {
	/// Locked amount.
	pub amount: Balance,
	/// Block at which the lock was created.
	pub lock_block: BlockNumber,
	/// Block at which the lock expires (subject to auto-renewal).
	pub expiry_block: BlockNumber,
	/// Whether the lock auto-renews while `Active`.
	pub auto_renew: bool,
	/// Current lifecycle status.
	pub status: ValidatorStatus,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// Currency used for validator stake locking.
		type Currency: Currency<Self::AccountId>;
	}

	/// Active validator lock records, keyed by account.
	#[pallet::storage]
	pub type ValidatorLocks<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		LockInfo<BalanceOf<T>, BlockNumberFor<T>>,
		OptionQuery,
	>;

	/// Equivocation cooldown deadline per account (block number).
	#[pallet::storage]
	pub type EquivocationCooldown<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, BlockNumberFor<T>, OptionQuery>;

	/// Consecutive offline session count per account.
	#[pallet::storage]
	pub type OfflineSessionCount<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, u32, ValueQuery>;

	#[pallet::event]
	// `deposit_event` is unused until dispatchables/hooks land in follow-up
	// issues (#012b onwards); declare it now so callers don't need to be
	// retro-fitted later.
	#[allow(dead_code)]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A validator locked stake. `[who, amount, expiry_block]`
		ValidatorLocked {
			who: T::AccountId,
			amount: BalanceOf<T>,
			expiry_block: BlockNumberFor<T>,
		},
		/// A validator requested voluntary exit.
		ValidatorExitRequested { who: T::AccountId },
		/// A validator was kicked (offline or equivocation).
		ValidatorKicked { who: T::AccountId, reason: KickReason },
		/// A validator's lock was released after expiry.
		LockReleased { who: T::AccountId, amount: BalanceOf<T> },
		/// A validator's lock was auto-renewed to a new expiry block.
		LockRenewed { who: T::AccountId, new_expiry_block: BlockNumberFor<T> },
	}

	/// Reason a validator was removed from the active set.
	#[derive(
		Clone,
		Copy,
		PartialEq,
		Eq,
		Debug,
		Encode,
		Decode,
		DecodeWithMemTracking,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub enum KickReason {
		/// Removed for being offline beyond the threshold.
		Offline,
		/// Removed for GRANDPA equivocation.
		Equivocation,
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Account is already a validator.
		AlreadyValidator,
		/// Account is not a registered validator.
		NotValidator,
		/// Provided stake amount is below the minimum threshold.
		InsufficientStake,
		/// Provided lock duration is below the minimum.
		LockDurationTooShort,
		/// Operation not permitted in the current validator status.
		InvalidStatus,
		/// Lock has not yet reached its expiry block.
		LockNotExpired,
		/// Account is currently within an equivocation cooldown.
		InCooldown,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}
}
