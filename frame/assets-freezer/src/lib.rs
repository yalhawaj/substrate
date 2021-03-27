// This file is part of Substrate.

// Copyright (C) 2017-2021 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! # Assets Freezer Pallet
//!
//! An extension pallet for use with the Assets pallet for allowing funds to be locked and reserved.

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]
/*
pub mod weights;
#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
pub mod mock;
#[cfg(test)]
mod tests;
*/
use sp_std::prelude::*;
use sp_runtime::{
	RuntimeDebug, TokenError, traits::{
		AtLeast32BitUnsigned, Zero, StaticLookup, Saturating, CheckedSub, CheckedAdd,
		StoredMapError,
	}
};
use codec::{Encode, Decode, HasCompact};
use frame_support::{ensure, dispatch::{DispatchError, DispatchResult}};
use frame_support::traits::{Currency, ReservableCurrency, BalanceStatus::Reserved, StoredMap};
use frame_support::traits::tokens::{WithdrawConsequence, DepositConsequence, fungibles};
use frame_system::Config as SystemConfig;
use pallet_assets::{Pallet as Assets, Config as AssetsConfig};

//pub use weights::WeightInfo;
pub use pallet::*;

type BalanceOf<T> = <<T as Config>::Assets as fungibles::Inspect<<T as SystemConfig>::AccountId>>::Balance;
type AssetIdOf<T> = <<T as Config>::Assets as fungibles::Inspect<<T as SystemConfig>::AccountId>>::AssetId;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::{
		dispatch::DispatchResult,
		pallet_prelude::*,
	};
	use frame_system::pallet_prelude::*;
	use super::*;

	/// The information concerning our freezing.
	#[derive(Eq, PartialEq, Clone, Encode, Decode, RuntimeDebug, Default)]
	pub struct FreezeData<Balance> {
		/// The amount of funds that have been reserved. The actual amount of funds held in reserve
		/// (and thus guaranteed of being unreserved) is this amount less `melted`.
		///
		/// If this `is_zero`, then the account may be deleted. If it is non-zero, then the assets
		/// pallet will attempt to keep the account alive by retaining the `minimum_balance` *plus*
		/// this number of funds in it.
		pub(super) reserved: Balance,
		/// The amount of funds that have melted (i.e. the account has been reduced despite them
		/// being reserved.
		pub(super) melted: Balance,
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	/// The module configuration trait.
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// The fungibles trait impl whose assets this reserves.
		type Assets: fungibles::Inspect<Self::AccountId>;

		/// Place to store the fast-access freeze data for the given asset/account.
		type Store: StoredMap<(AssetIdOf<Self>, Self::AccountId), FreezeData<BalanceOf<Self>>>;

//		/// Weight information for extrinsics in this pallet.
//		type WeightInfo: WeightInfo;
	}

	//
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	#[pallet::metadata(T::AccountId = "AccountId", BalanceOf<T> = "Balance", AssetIdOf<T> = "AssetId")]
	pub enum Event<T: Config> {
		/// An asset has been reserved.
		/// \[asset, who, amount\]
		Reserved(AssetIdOf<T>, T::AccountId, BalanceOf<T>),
		/// An asset has been unreserved.
		/// \[asset, who, amount\]
		Unreserved(AssetIdOf<T>, T::AccountId, BalanceOf<T>),
	}

	// No new errors?
	#[pallet::error]
	pub enum Error<T> {
		/// The origin account is frozen.
		Frozen,
	}

	// No hooks.
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	// Only admin calls.
	#[pallet::call]
	impl<T: Config> Pallet<T> {}
}

// The main implementation block for the module.
impl<T: Config> Pallet<T> {
}

impl<T: Config> pallet_assets::FrozenBalance<AssetIdOf<T>, T::AccountId, BalanceOf<T>> for Pallet<T> {
	fn frozen_balance(id: AssetIdOf<T>, who: &T::AccountId) -> Option<BalanceOf<T>> {
		let f = T::Store::get(&(id, who.clone()));
		if f.reserved.is_zero() { None } else { Some(f.reserved) }
	}
	fn melted(id: AssetIdOf<T>, who: &T::AccountId, amount: BalanceOf<T>) {
		// Just bump melted balance, assuming that the account still exists.
		let r = T::Store::mutate(&(id, who.clone()), |extra|
			extra.melted = extra.melted.saturating_add(amount)
		);
		debug_assert!(r.is_ok(), "account should still exist when melted.");
	}
	fn died(_: AssetIdOf<T>, _: &T::AccountId) {
		// Eventually need to remove lock named reserve/lock info.
	}
}

impl<T: Config> fungibles::Inspect<<T as SystemConfig>::AccountId> for Pallet<T> {
	type AssetId = AssetIdOf<T>;
	type Balance = BalanceOf<T>;
	fn total_issuance(asset: AssetIdOf<T>) -> BalanceOf<T> {
		T::Assets::total_issuance(asset)
	}
	fn minimum_balance(asset: AssetIdOf<T>) -> BalanceOf<T> {
		T::Assets::minimum_balance(asset)
	}
	fn balance(asset: AssetIdOf<T>, who: &T::AccountId) -> BalanceOf<T> {
		T::Assets::balance(asset, who)
	}
	fn withdrawable_balance(asset: AssetIdOf<T>, who: &T::AccountId) -> BalanceOf<T> {
		T::Assets::withdrawable_balance(asset, who)
	}
	fn can_deposit(asset: AssetIdOf<T>, who: &T::AccountId, amount: BalanceOf<T>)
		-> DepositConsequence
	{
		T::Assets::can_deposit(asset, who, amount)
	}
	fn can_withdraw(
		asset: AssetIdOf<T>,
		who: &T::AccountId,
		amount: BalanceOf<T>,
	) -> WithdrawConsequence<BalanceOf<T>> {
		T::Assets::can_withdraw(asset, who, amount)
	}
}

impl<T: Config> fungibles::InspectReserve<<T as SystemConfig>::AccountId> for Pallet<T> {
	fn reserved_balance(asset: AssetIdOf<T>, who: &T::AccountId) -> BalanceOf<T> {
		T::Store::get(&(asset, who.clone())).reserved
	}
	fn can_reserve(asset: AssetIdOf<T>, who: &T::AccountId, amount: BalanceOf<T>) -> bool {
		// If we can withdraw without destroying the account, then we're good.
		<Self as fungibles::Inspect<T::AccountId>>::can_withdraw(asset, who, amount) == WithdrawConsequence::Success
	}
}

//impl<T: Config> fungibles::MutateReserve<<T as SystemConfig>::AccountId> for Pallet<T> {
//}
