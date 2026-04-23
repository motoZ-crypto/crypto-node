//! # Pallet Validator
//!
//! Manages the full lifecycle of validators: lock, auto-renewal, exit,
//! and kick. This crate currently provides the storage, event, and error
//! skeleton.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

extern crate alloc;

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::traits::{Currency, LockableCurrency};
use scale_info::TypeInfo;
use sp_runtime::traits::{Saturating, Zero};

/// Balance type alias derived from the configured `Currency`.
pub type BalanceOf<T> = <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId,>>::Balance;

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
	pub amount: Balance,
	pub lock_block: BlockNumber,
	pub expiry_block: BlockNumber,
	pub status: ValidatorStatus,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{
		pallet_prelude::*,
		traits::{LockIdentifier, WithdrawReasons},
	};
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// Currency used for validator stake locking.
		type Currency: LockableCurrency<Self::AccountId, Moment = BlockNumberFor<Self>>;

		/// Amount to lock when registering as a validator.
		#[pallet::constant]
		type LockAmount: Get<BalanceOf<Self>>;

		/// Lock duration (in blocks) applied at registration and on each renewal.
		#[pallet::constant]
		type LockDuration: Get<BlockNumberFor<Self>>;

		/// Lock identifier used when calling `set_lock` on the underlying currency.
		#[pallet::constant]
		type LockId: Get<LockIdentifier>;

		/// Upper bound for the number of pending/active validators tracked in storage.
		#[pallet::constant]
		type MaxValidators: Get<u32>;

		/// Interval (in blocks) between auto-renewal sweeps.
		#[pallet::constant]
		type RenewInterval: Get<BlockNumberFor<Self>>;
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

	/// Validators waiting to be promoted into the active set at the next session boundary.
	#[pallet::storage]
	pub type PendingValidators<T: Config> = StorageValue<_, BoundedVec<T::AccountId, T::MaxValidators>, ValueQuery>;

	/// Rejoin cooldown deadline per account (block number).
	#[pallet::storage]
	pub type RejoinCooldown<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, BlockNumberFor<T>, OptionQuery>;

	/// Consecutive offline session count per account.
	#[pallet::storage]
	pub type OfflineSessionCount<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, u32, ValueQuery>;

	#[pallet::event]
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
		/// Operation not permitted in the current validator status.
		InvalidStatus,
		/// Lock has not yet reached its expiry block.
		LockNotExpired,
		/// Account is currently within an equivocation cooldown.
		InCooldown,
		/// Account does not have enough free balance to cover the configured lock.
		InsufficientBalance,
		/// The pending validator queue is full.
		TooManyValidators,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(now: BlockNumberFor<T>) -> Weight {
			let interval = T::RenewInterval::get();
			if interval.is_zero() {
				return Weight::from_parts(0, 0);
			}

			let duration = T::LockDuration::get();
			let mut to_renew: alloc::vec::Vec<T::AccountId> = alloc::vec::Vec::new();
			for (who, info) in ValidatorLocks::<T>::iter() {
				if info.status != ValidatorStatus::Active {
					continue;
				}
				let remaining = info.expiry_block.saturating_sub(now);
				let elapsed_window = duration.saturating_sub(remaining);
				if elapsed_window >= interval {
					to_renew.push(who);
				}
			}

			let count = to_renew.len() as u64;
			for who in to_renew {
				ValidatorLocks::<T>::mutate(&who, |maybe_info| {
					if let Some(info) = maybe_info {
						info.expiry_block = now.saturating_add(duration);
						Self::deposit_event(Event::ValidatorLocked {
							who: who.clone(),
							amount: info.amount,
							expiry_block: info.expiry_block,
						});
					}
				});
			}

			// Rough weight: one read per lock scanned + one write per renewal.
			T::DbWeight::get().reads_writes(count, count)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Lock the configured stake amount for the configured duration and
		/// queue the caller into [`PendingValidators`] for the next session.
		///
		/// The locked amount and duration are taken from `Config::LockAmount`
		/// and `Config::LockDuration` respectively; callers do not choose them.
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(50_000_000, 0))]
		pub fn lock(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;

			ensure!(!ValidatorLocks::<T>::contains_key(&who), Error::<T>::AlreadyValidator);

			let now = frame_system::Pallet::<T>::block_number();
			if let Some(deadline) = RejoinCooldown::<T>::get(&who) {
				if deadline > now {
					return Err(Error::<T>::InCooldown.into());
				}
				RejoinCooldown::<T>::remove(&who);
			}

			let amount = T::LockAmount::get();
			let duration = T::LockDuration::get();

			ensure!(
				T::Currency::free_balance(&who) >= amount,
				Error::<T>::InsufficientBalance,
			);

			let expiry_block = now.saturating_add(duration);

			PendingValidators::<T>::try_mutate(|queue| -> DispatchResult {
				ensure!(!queue.iter().any(|a| a == &who), Error::<T>::AlreadyValidator);
				queue
					.try_push(who.clone())
					.map_err(|_| Error::<T>::TooManyValidators)?;
				Ok(())
			})?;

			T::Currency::set_lock(
				T::LockId::get(),
				&who,
				amount,
				WithdrawReasons::all(),
			);

			ValidatorLocks::<T>::insert(
				&who,
				LockInfo {
					amount,
					lock_block: now,
					expiry_block,
					status: ValidatorStatus::Active,
				},
			);

			Self::deposit_event(Event::ValidatorLocked { who, amount, expiry_block });
			Ok(())
		}
	}
}
