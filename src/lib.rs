#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::unused_unit)]

use codec::Codec;
use frame_support::{
	pallet_prelude::*,
	traits::{
		Currency as SetheumCurrency, ExistenceRequirement, Get, 
		LockableCurrency as SetheumLockableCurrency,
		ReservableCurrency as SetheumReservableCurrency, WithdrawReasons,
	},
};
use frame_system::{ensure_root, ensure_signed, pallet_prelude::*};
use stp258_traits::{
	account::MergeAccount,
	arithmetic::{Signed, SimpleArithmetic},
	BalanceStatus, SerpMarket, Stp258Asset, Stp258AssetExtended, Stp258AssetLockable, Stp258AssetReservable,
	LockIdentifier, Stp258Currency, Stp258CurrencyExtended, Stp258CurrencyReservable, Stp258CurrencyLockable,
};
use orml_utilities::with_transaction_result;
use sp_runtime::{
	traits::{CheckedSub, MaybeSerializeDeserialize, StaticLookup, Zero},
	DispatchError, DispatchResult,
};
use sp_std::{
	convert::{TryFrom, TryInto},
	fmt::Debug,
	marker, result,
};

mod default_weight;
mod mock;
mod tests;

pub use module::*;

#[frame_support::pallet]
pub mod module {
	use super::*;

	pub trait WeightInfo {
		fn transfer_non_native_currency() -> Weight;
		fn transfer_native_currency() -> Weight;
		fn update_balance_non_native_currency() -> Weight;
		fn update_balance_native_currency_creating() -> Weight;
		fn update_balance_native_currency_killing() -> Weight;
	}

	pub(crate) type BalanceOf<T> =
		<<T as Config>::Stp258Currency as Stp258Currency<<T as frame_system::Config>::AccountId>>::Balance;
	pub(crate) type CurrencyIdOf<T> =
		<<T as Config>::Stp258Currency as Stp258Currency<<T as frame_system::Config>::AccountId>>::CurrencyId;
	pub(crate) type AmountOf<T> =
		<<T as Config>::Stp258Currency as Stp258CurrencyExtended<<T as frame_system::Config>::AccountId>>::Amount;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		type Stp258Currency: MergeAccount<Self::AccountId>
			+ Stp258CurrencyExtended<Self::AccountId>
			+ Stp258CurrencyLockable<Self::AccountId>
			+ Stp258CurrencyReservable<Self::AccountId>
			+ SerpMarket<Self::AccountId>;

		type Stp258Native: Stp258AssetExtended<Self::AccountId, Balance = BalanceOf<Self>, Amount = AmountOf<Self>>
			+ Stp258AssetLockable<Self::AccountId, Balance = BalanceOf<Self>>
			+ Stp258AssetReservable<Self::AccountId, Balance = BalanceOf<Self>>;

		#[pallet::constant]
		type GetStp258NativeId: Get<CurrencyIdOf<Self>>;

		#[pallet::constant]
		type GetSerpNativeId: Get<CurrencyIdOf<Self>>;

		/// Weight information for extrinsics in this module.
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Unable to convert the Amount type into Balance.
		AmountIntoBalanceFailed,
		/// Balance is too low.
		BalanceTooLow,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Currency transfer success. [currency_id, from, to, amount]
		Transferred(CurrencyIdOf<T>, T::AccountId, T::AccountId, BalanceOf<T>),
		/// Update balance success. [currency_id, who, amount]
		BalanceUpdated(CurrencyIdOf<T>, T::AccountId, AmountOf<T>),
		/// Deposit success. [currency_id, who, amount]
		Deposited(CurrencyIdOf<T>, T::AccountId, BalanceOf<T>),
		/// Withdraw success. [currency_id, who, amount]
		Withdrawn(CurrencyIdOf<T>, T::AccountId, BalanceOf<T>),
		/// Contract Supply Success. [currency_id, contract_by]
		SerpedDownSupply(CurrencyIdOf<T>, BalanceOf<T>),
		/// Expand Supply Success. [currency_id, expand_by]
		SerpedUpSupply(CurrencyIdOf<T>, BalanceOf<T>),
	}

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::hooks]
	impl<T: Config> Hooks<T::BlockNumber> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Transfer some balance to another account under `currency_id`.
		///
		/// The dispatch origin for this call must be `Signed` by the
		/// transactor.
		#[pallet::weight(T::WeightInfo::transfer_non_native_currency())]
		pub fn transfer(
			origin: OriginFor<T>,
			dest: <T::Lookup as StaticLookup>::Source,
			currency_id: CurrencyIdOf<T>,
			#[pallet::compact] amount: BalanceOf<T>,
		) -> DispatchResultWithPostInfo {
			let from = ensure_signed(origin)?;
			let to = T::Lookup::lookup(dest)?;
			<Self as Stp258Currency<T::AccountId>>::transfer(currency_id, &from, &to, amount)?;
			Ok(().into())
		}

		/// Transfer some native currency to another account.
		///
		/// The dispatch origin for this call must be `Signed` by the
		/// transactor.
		#[pallet::weight(T::WeightInfo::transfer_native_currency())]
		pub fn transfer_native_currency(
			origin: OriginFor<T>,
			dest: <T::Lookup as StaticLookup>::Source,
			#[pallet::compact] amount: BalanceOf<T>,
		) -> DispatchResultWithPostInfo {
			let from = ensure_signed(origin)?;
			let to = T::Lookup::lookup(dest)?;
			T::Stp258Native::transfer(&from, &to, amount)?;

			Self::deposit_event(Event::Transferred(T::GetStp258NativeId::get(), from, to, amount));
			Ok(().into())
		}

		/// update amount of account `who` under `currency_id`.
		///
		/// The dispatch origin of this call must be _Root_.
		#[pallet::weight(T::WeightInfo::update_balance_non_native_currency())]
		pub fn update_balance(
			origin: OriginFor<T>,
			who: <T::Lookup as StaticLookup>::Source,
			currency_id: CurrencyIdOf<T>,
			amount: AmountOf<T>,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			let dest = T::Lookup::lookup(who)?;
			<Self as Stp258CurrencyExtended<T::AccountId>>::update_balance(currency_id, &dest, amount)?;
			Ok(().into())
		}
	}
}

/// Adapt SerpTes traits implementation to `SerpTes` in the `Stp258Standard`.
pub struct SerpTesAdapter<BlockNumber, CurrencyId, Balance>(marker::PhantomData<(BlockNumber, CurrencyId, Balance)>);

// Adapt `frame_support::traits::Currency`
impl<BlockNumber, CurrencyId, Balance> SerpTes<AccountId>
	for SerpTesAdapter<BlockNumber, CurrencyId, Balance>
where
    BlockNumber: Blocknumber,
    CurrencyId: Parameter + Member + Copy + MaybeSerializeDeserialize,
    Balance: AtLeast32BitUnsigned + FullCodec + Copy + MaybeSerializeDeserialize + Debug + Default;
	SerpTes: SerpTes<BlockNumber>,
{	
	fn adjustment_frequency() -> Result<(), &'static str> {
		T::AdjustmentFrequency::get()
	}

	fn on_serp_initialize(now: T::BlockNumber, sett_price: u64, sett_currency_id: T::CurrencyId; jusd_price: u64; jusd_currency_id: T::CurrencyId) -> DispatchResult {

		let sett_price_on_block = Self::on_block_with_price(now, sett_price, sett_currency_id).unwrap_or_else(|e| {
			native::error!("could not adjust supply: {:?}", e);
		});
		let jusd_price_on_block = Self::on_block_with_price(now, jusd_price, jusd_currency_id).unwrap_or_else(|e| {
			native::error!("could not adjust supply: {:?}", e);
		});

		Self::on_block_with_price(now, price).unwrap_or_else(|e| {
			native::error!("could not adjust supply: {:?}", e);
		});
	}

	/// Calculate the amount of supply change from a fraction.
	fn supply_change(currency_id:  Self::CurrencyId, new_price: Self::Balance) -> Self::Balance {
		let base_unit = T::GetBaseUnit::get(&currency_id);
		let supply = <Self as Stp258Currency<T::AccountId>>::total_issuance(currency_id);
		let fraction = new_price * supply;
		let fractioned = fraction / base_unit;
		fractioned - supply;
	}

	/// Contracts or expands the currency supply based on conditions.
	fn on_block_with_price(block: &T::Blocknumber, price: Self::Balance, currency_id: Self::CurrencyId) -> DispatchResult {
		// This can be changed to only correct for small or big price swings.
		let serp_elast_adjuster = T::AdjustmentFrequency::get();
		if block % serp_elast_adjuster == 0.into() {
			Self::serp_elast(currency_id, price)
		} else {
			Ok(())
		}
	}

	/// Expands (if the price is too high) or contracts (if the price is too low) the SettCurrency supply.
	///
	/// **Weight:**
	/// - complexity: `O(S + C)`
	///   - `S` being the complexity of executing either `expand_supply` or `contract_supply`
	///   - `C` being a constant amount of storage reads for SettCurrency supply
	/// - DB access:
	///   - 1 read for total_issuance
	///   - execute `expand_supply` OR execute `contract_supply` which have DB accesses
	#[weight = 0]
	fn serp_elast(currency_id: CurrencyId, price: Balance) -> DispatchResult {
		let base_unit = T::GetBaseUnit;
		match price {
			0 => {
				native::error!("currency price is zero!");
				return Err(DispatchError::from(Error::<T>::ZeroPrice));
			}
			price if price > base_unit => {
				// safe from underflow because `price` is checked to be less than `GetBaseUnit`
				let expand_by = Self::supply_change(currency_id, price);
				<Self as Stp258Currency<_>>expand_supply(currency_id, expand_by, price)?;
			}
			price if price < base_unit => {
				// safe from underflow because `price` is checked to be greater than `GetBaseUnit`
				let contract_by = Self::supply_change(currency_id, price);
				<Self as Stp258Currency<_>>contract_supply(currency_id, expand_by, price)?;
			}
			_ => {
				native::info!("settcurrency price is equal to base as is desired --> nothing to do");
			}
		}
		Ok(())
	}
}

impl<T: Config> SerpMarket<T::AccountId> for Pallet<T> {

	fn expand_supply(
		native_currency_id: Self::CurrencyId,
		stable_currency_id: Self::CurrencyId,
		expand_by: Self::Balance,
		pay_by_quoted: Self::Balance,
		serpers: &T::AccountId,
	) -> DispatchResult {
		if expand_by.is_zero() || stable_currency_id == native_currency_id {
			return Ok(());
		}
        if native_currency_id == T::GetSerpNativeId::get() {
			T::Stp258Currency::expand_supply(native_currency_id, stable_currency_id, expand_by, pay_by_quoted, serpers)?;
		}
        Self::deposit_event(Event::SerpedUpSupply(stable_currency_id, expand_by));
        Ok(())
	}

	fn contract_supply(
        native_currency_id: Self::CurrencyId,
        stable_currency_id: Self::CurrencyId,
        contract_by: Self::Balance,
        pay_by_quoted: Self::Balance,
		serpers: &T::AccountId,
    ) -> DispatchResult {
		if contract_by.is_zero() || stable_currency_id == native_currency_id {
			return Ok(());
		}
        if native_currency_id == T::GetSerpNativeId::get() {
			T::Stp258Currency::contract_supply(native_currency_id, stable_currency_id, contract_by, pay_by_quoted, serpers)?;
		}
        Self::deposit_event(Event::SerpedDownSupply(stable_currency_id, contract_by));
        Ok(())
    }
}

impl<T: Config> Stp258Currency<T::AccountId> for Pallet<T> {
	type CurrencyId = CurrencyIdOf<T>;
	type Balance = BalanceOf<T>;

	fn base_unit(currency_id: Self::CurrencyId) -> Self::Balance {
		if currency_id == T::GetStp258NativeId::get() {
			T::Stp258Native::minimum_balance()
		} else {
			T::Stp258Currency::base_unit(currency_id)
		}
	}

	fn minimum_balance(currency_id: Self::CurrencyId) -> Self::Balance {
		if currency_id == T::GetStp258NativeId::get() {
			T::Stp258Native::minimum_balance()
		} else {
			T::Stp258Currency::minimum_balance(currency_id)
		}
	}

	fn total_issuance(currency_id: Self::CurrencyId) -> Self::Balance {
		if currency_id == T::GetStp258NativeId::get() {
			T::Stp258Native::total_issuance()
		} else {
			T::Stp258Currency::total_issuance(currency_id)
		}
	}

	fn total_balance(currency_id: Self::CurrencyId, who: &T::AccountId) -> Self::Balance {
		if currency_id == T::GetStp258NativeId::get() {
			T::Stp258Native::total_balance(who)
		} else {
			T::Stp258Currency::total_balance(currency_id, who)
		}
	}

	fn free_balance(currency_id: Self::CurrencyId, who: &T::AccountId) -> Self::Balance {
		if currency_id == T::GetStp258NativeId::get() {
			T::Stp258Native::free_balance(who)
		} else {
			T::Stp258Currency::free_balance(currency_id, who)
		}
	}

	fn ensure_can_withdraw(currency_id: Self::CurrencyId, who: &T::AccountId, amount: Self::Balance) -> DispatchResult {
		if currency_id == T::GetStp258NativeId::get() {
			T::Stp258Native::ensure_can_withdraw(who, amount)
		} else {
			T::Stp258Currency::ensure_can_withdraw(currency_id, who, amount)
		}
	}

	fn transfer(
		currency_id: Self::CurrencyId,
		from: &T::AccountId,
		to: &T::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		if amount.is_zero() || from == to {
			return Ok(());
		}
		if currency_id == T::GetStp258NativeId::get() {
			T::Stp258Native::transfer(from, to, amount)?;
		} else {
			T::Stp258Currency::transfer(currency_id, from, to, amount)?;
		}
		Self::deposit_event(Event::Transferred(currency_id, from.clone(), to.clone(), amount));
		Ok(())
	}

	fn deposit(currency_id: Self::CurrencyId, who: &T::AccountId, amount: Self::Balance) -> DispatchResult {
		if amount.is_zero() {
			return Ok(());
		}
		if currency_id == T::GetStp258NativeId::get() {
			T::Stp258Native::deposit(who, amount)?;
		} else {
			T::Stp258Currency::deposit(currency_id, who, amount)?;
		}
		Self::deposit_event(Event::Deposited(currency_id, who.clone(), amount));
		Ok(())
	}

	fn withdraw(currency_id: Self::CurrencyId, who: &T::AccountId, amount: Self::Balance) -> DispatchResult {
		if amount.is_zero() {
			return Ok(());
		}
		if currency_id == T::GetStp258NativeId::get() {
			T::Stp258Native::withdraw(who, amount)?;
		} else {
			T::Stp258Currency::withdraw(currency_id, who, amount)?;
		}
		Self::deposit_event(Event::Withdrawn(currency_id, who.clone(), amount));
		Ok(())
	}

	fn can_slash(currency_id: Self::CurrencyId, who: &T::AccountId, amount: Self::Balance) -> bool {
		if currency_id == T::GetStp258NativeId::get() {
			T::Stp258Native::can_slash(who, amount)
		} else {
			T::Stp258Currency::can_slash(currency_id, who, amount)
		}
	}

	fn slash(currency_id: Self::CurrencyId, who: &T::AccountId, amount: Self::Balance) -> Self::Balance {
		if currency_id == T::GetStp258NativeId::get() {
			T::Stp258Native::slash(who, amount)
		} else {
			T::Stp258Currency::slash(currency_id, who, amount)
		}
	}
}

impl<T: Config> Stp258CurrencyExtended<T::AccountId> for Pallet<T> {
	type Amount = AmountOf<T>;

	fn update_balance(currency_id: Self::CurrencyId, who: &T::AccountId, by_amount: Self::Amount) -> DispatchResult {
		if currency_id == T::GetStp258NativeId::get() {
			T::Stp258Native::update_balance(who, by_amount)?;
		} else {
			T::Stp258Currency::update_balance(currency_id, who, by_amount)?;
		}
		Self::deposit_event(Event::BalanceUpdated(currency_id, who.clone(), by_amount));
		Ok(())
	}
}

impl<T: Config> Stp258CurrencyLockable<T::AccountId> for Pallet<T> {
	type Moment = T::BlockNumber;

	fn set_lock(
		lock_id: LockIdentifier,
		currency_id: Self::CurrencyId,
		who: &T::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		if currency_id == T::GetStp258NativeId::get() {
			T::Stp258Native::set_lock(lock_id, who, amount)
		} else {
			T::Stp258Currency::set_lock(lock_id, currency_id, who, amount)
		}
	}

	fn extend_lock(
		lock_id: LockIdentifier,
		currency_id: Self::CurrencyId,
		who: &T::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		if currency_id == T::GetStp258NativeId::get() {
			T::Stp258Native::extend_lock(lock_id, who, amount)
		} else {
			T::Stp258Currency::extend_lock(lock_id, currency_id, who, amount)
		}
	}

	fn remove_lock(lock_id: LockIdentifier, currency_id: Self::CurrencyId, who: &T::AccountId) -> DispatchResult {
		if currency_id == T::GetStp258NativeId::get() {
			T::Stp258Native::remove_lock(lock_id, who)
		} else {
			T::Stp258Currency::remove_lock(lock_id, currency_id, who)
		}
	}
}

impl<T: Config> Stp258CurrencyReservable<T::AccountId> for Pallet<T> {
	fn can_reserve(currency_id: Self::CurrencyId, who: &T::AccountId, value: Self::Balance) -> bool {
		if currency_id == T::GetStp258NativeId::get() {
			T::Stp258Native::can_reserve(who, value)
		} else {
			T::Stp258Currency::can_reserve(currency_id, who, value)
		}
	}

	fn slash_reserved(currency_id: Self::CurrencyId, who: &T::AccountId, value: Self::Balance) -> Self::Balance {
		if currency_id == T::GetStp258NativeId::get() {
			T::Stp258Native::slash_reserved(who, value)
		} else {
			T::Stp258Currency::slash_reserved(currency_id, who, value)
		}
	}

	fn reserved_balance(currency_id: Self::CurrencyId, who: &T::AccountId) -> Self::Balance {
		if currency_id == T::GetStp258NativeId::get() {
			T::Stp258Native::reserved_balance(who)
		} else {
			T::Stp258Currency::reserved_balance(currency_id, who)
		}
	}

	fn reserve(currency_id: Self::CurrencyId, who: &T::AccountId, value: Self::Balance) -> DispatchResult {
		if currency_id == T::GetStp258NativeId::get() {
			T::Stp258Native::reserve(who, value)
		} else {
			T::Stp258Currency::reserve(currency_id, who, value)
		}
	}

	fn unreserve(currency_id: Self::CurrencyId, who: &T::AccountId, value: Self::Balance) -> Self::Balance {
		if currency_id == T::GetStp258NativeId::get() {
			T::Stp258Native::unreserve(who, value)
		} else {
			T::Stp258Currency::unreserve(currency_id, who, value)
		}
	}

	fn repatriate_reserved(
		currency_id: Self::CurrencyId,
		slashed: &T::AccountId,
		beneficiary: &T::AccountId,
		value: Self::Balance,
		status: BalanceStatus,
	) -> result::Result<Self::Balance, DispatchError> {
		if currency_id == T::GetStp258NativeId::get() {
			T::Stp258Native::repatriate_reserved(slashed, beneficiary, value, status)
		} else {
			T::Stp258Currency::repatriate_reserved(currency_id, slashed, beneficiary, value, status)
		}
	}
}

pub struct Currency<T, GetCurrencyId>(marker::PhantomData<T>, marker::PhantomData<GetCurrencyId>);

impl<T, GetCurrencyId> Stp258Asset<T::AccountId> for Currency<T, GetCurrencyId>
where
	T: Config,
	GetCurrencyId: Get<CurrencyIdOf<T>>,
{
	type Balance = BalanceOf<T>;

	fn minimum_balance() -> Self::Balance {
		<Pallet<T>>::minimum_balance(GetCurrencyId::get())
	}

	fn total_issuance() -> Self::Balance {
		<Pallet<T>>::total_issuance(GetCurrencyId::get())
	}

	fn total_balance(who: &T::AccountId) -> Self::Balance {
		<Pallet<T>>::total_balance(GetCurrencyId::get(), who)
	}

	fn free_balance(who: &T::AccountId) -> Self::Balance {
		<Pallet<T>>::free_balance(GetCurrencyId::get(), who)
	}

	fn ensure_can_withdraw(who: &T::AccountId, amount: Self::Balance) -> DispatchResult {
		<Pallet<T>>::ensure_can_withdraw(GetCurrencyId::get(), who, amount)
	}

	fn transfer(from: &T::AccountId, to: &T::AccountId, amount: Self::Balance) -> DispatchResult {
		<Pallet<T> as Stp258Currency<T::AccountId>>::transfer(GetCurrencyId::get(), from, to, amount)
	}

	fn deposit(who: &T::AccountId, amount: Self::Balance) -> DispatchResult {
		<Pallet<T>>::deposit(GetCurrencyId::get(), who, amount)
	}

	fn withdraw(who: &T::AccountId, amount: Self::Balance) -> DispatchResult {
		<Pallet<T>>::withdraw(GetCurrencyId::get(), who, amount)
	}

	fn can_slash(who: &T::AccountId, amount: Self::Balance) -> bool {
		<Pallet<T>>::can_slash(GetCurrencyId::get(), who, amount)
	}

	fn slash(who: &T::AccountId, amount: Self::Balance) -> Self::Balance {
		<Pallet<T>>::slash(GetCurrencyId::get(), who, amount)
	}
}

impl<T, GetCurrencyId> Stp258AssetExtended<T::AccountId> for Currency<T, GetCurrencyId>
where
	T: Config,
	GetCurrencyId: Get<CurrencyIdOf<T>>,
{
	type Amount = AmountOf<T>;

	fn update_balance(who: &T::AccountId, by_amount: Self::Amount) -> DispatchResult {
		<Pallet<T> as Stp258CurrencyExtended<T::AccountId>>::update_balance(GetCurrencyId::get(), who, by_amount)
	}
}

impl<T, GetCurrencyId> Stp258AssetLockable<T::AccountId> for Currency<T, GetCurrencyId>
where
	T: Config,
	GetCurrencyId: Get<CurrencyIdOf<T>>,
{
	type Moment = T::BlockNumber;

	fn set_lock(lock_id: LockIdentifier, who: &T::AccountId, amount: Self::Balance) -> DispatchResult {
		<Pallet<T> as Stp258CurrencyLockable<T::AccountId>>::set_lock(lock_id, GetCurrencyId::get(), who, amount)
	}

	fn extend_lock(lock_id: LockIdentifier, who: &T::AccountId, amount: Self::Balance) -> DispatchResult {
		<Pallet<T> as Stp258CurrencyLockable<T::AccountId>>::extend_lock(lock_id, GetCurrencyId::get(), who, amount)
	}

	fn remove_lock(lock_id: LockIdentifier, who: &T::AccountId) -> DispatchResult {
		<Pallet<T> as Stp258CurrencyLockable<T::AccountId>>::remove_lock(lock_id, GetCurrencyId::get(), who)
	}
}

impl<T, GetCurrencyId> Stp258AssetReservable<T::AccountId> for Currency<T, GetCurrencyId>
where
	T: Config,
	GetCurrencyId: Get<CurrencyIdOf<T>>,
{
	fn can_reserve(who: &T::AccountId, value: Self::Balance) -> bool {
		<Pallet<T> as Stp258CurrencyReservable<T::AccountId>>::can_reserve(GetCurrencyId::get(), who, value)
	}

	fn slash_reserved(who: &T::AccountId, value: Self::Balance) -> Self::Balance {
		<Pallet<T> as Stp258CurrencyReservable<T::AccountId>>::slash_reserved(GetCurrencyId::get(), who, value)
	}

	fn reserved_balance(who: &T::AccountId) -> Self::Balance {
		<Pallet<T> as Stp258CurrencyReservable<T::AccountId>>::reserved_balance(GetCurrencyId::get(), who)
	}

	fn reserve(who: &T::AccountId, value: Self::Balance) -> DispatchResult {
		<Pallet<T> as Stp258CurrencyReservable<T::AccountId>>::reserve(GetCurrencyId::get(), who, value)
	}

	fn unreserve(who: &T::AccountId, value: Self::Balance) -> Self::Balance {
		<Pallet<T> as Stp258CurrencyReservable<T::AccountId>>::unreserve(GetCurrencyId::get(), who, value)
	}

	fn repatriate_reserved(
		slashed: &T::AccountId,
		beneficiary: &T::AccountId,
		value: Self::Balance,
		status: BalanceStatus,
	) -> result::Result<Self::Balance, DispatchError> {
		<Pallet<T> as Stp258CurrencyReservable<T::AccountId>>::repatriate_reserved(
			GetCurrencyId::get(),
			slashed,
			beneficiary,
			value,
			status,
		)
	}
}

pub type Stp258NativeOf<T> = Currency<T, <T as Config>::GetStp258NativeId>;

/// Adapt other currency traits implementation to `Stp258Asset`.
pub struct Stp258AssetAdapter<T, Currency, Amount, Moment>(marker::PhantomData<(T, Currency, Amount, Moment)>);

type PalletBalanceOf<A, Currency> = <Currency as SetheumCurrency<A>>::Balance;

// Adapt `frame_support::traits::Currency`
impl<T, AccountId, Currency, Amount, Moment> Stp258Asset<AccountId>
	for Stp258AssetAdapter<T, Currency, Amount, Moment>
where
	Currency: SetheumCurrency<AccountId>,
	T: Config,
{
	type Balance = PalletBalanceOf<AccountId, Currency>;

	fn minimum_balance() -> Self::Balance {
		Currency::minimum_balance()
	}

	fn total_issuance() -> Self::Balance {
		Currency::total_issuance()
	}

	fn total_balance(who: &AccountId) -> Self::Balance {
		Currency::total_balance(who)
	}

	fn free_balance(who: &AccountId) -> Self::Balance {
		Currency::free_balance(who)
	}

	fn ensure_can_withdraw(who: &AccountId, amount: Self::Balance) -> DispatchResult {
		let new_balance = Self::free_balance(who)
			.checked_sub(&amount)
			.ok_or(Error::<T>::BalanceTooLow)?;

		Currency::ensure_can_withdraw(who, amount, WithdrawReasons::all(), new_balance)
	}

	fn transfer(from: &AccountId, to: &AccountId, amount: Self::Balance) -> DispatchResult {
		Currency::transfer(from, to, amount, ExistenceRequirement::AllowDeath)
	}

	fn deposit(who: &AccountId, amount: Self::Balance) -> DispatchResult {
		let _ = Currency::deposit_creating(who, amount);
		Ok(())
	}

	fn withdraw(who: &AccountId, amount: Self::Balance) -> DispatchResult {
		Currency::withdraw(who, amount, WithdrawReasons::all(), ExistenceRequirement::AllowDeath).map(|_| ())
	}

	fn can_slash(who: &AccountId, amount: Self::Balance) -> bool {
		Currency::can_slash(who, amount)
	}

	fn slash(who: &AccountId, amount: Self::Balance) -> Self::Balance {
		let (_, gap) = Currency::slash(who, amount);
		gap
	}
}

// Adapt `frame_support::traits::Currency`
impl<T, AccountId, Currency, Amount, Moment> Stp258AssetExtended<AccountId>
	for Stp258AssetAdapter<T, Currency, Amount, Moment>
where
	Amount: Signed
		+ TryInto<PalletBalanceOf<AccountId, Currency>>
		+ TryFrom<PalletBalanceOf<AccountId, Currency>>
		+ SimpleArithmetic
		+ Codec
		+ Copy
		+ MaybeSerializeDeserialize
		+ Debug
		+ Default,
	Currency: SetheumCurrency<AccountId>,
	T: Config,
{
	type Amount = Amount;

	fn update_balance(who: &AccountId, by_amount: Self::Amount) -> DispatchResult {
		let by_balance = by_amount
			.abs()
			.try_into()
			.map_err(|_| Error::<T>::AmountIntoBalanceFailed)?;
		if by_amount.is_positive() {
			Self::deposit(who, by_balance)
		} else {
			Self::withdraw(who, by_balance)
		}
	}
}

// Adapt `frame_support::traits::LockableCurrency`
impl<T, AccountId, Currency, Amount, Moment> Stp258AssetLockable<AccountId>
	for Stp258AssetAdapter<T, Currency, Amount, Moment>
where
	Currency: SetheumLockableCurrency<AccountId>,
	T: Config,
{
	type Moment = Moment;

	fn set_lock(lock_id: LockIdentifier, who: &AccountId, amount: Self::Balance) -> DispatchResult {
		Currency::set_lock(lock_id, who, amount, WithdrawReasons::all());
		Ok(())
	}

	fn extend_lock(lock_id: LockIdentifier, who: &AccountId, amount: Self::Balance) -> DispatchResult {
		Currency::extend_lock(lock_id, who, amount, WithdrawReasons::all());
		Ok(())
	}

	fn remove_lock(lock_id: LockIdentifier, who: &AccountId) -> DispatchResult {
		Currency::remove_lock(lock_id, who);
		Ok(())
	}
}

// Adapt `frame_support::traits::ReservableCurrency`
impl<T, AccountId, Currency, Amount, Moment> Stp258AssetReservable<AccountId>
	for Stp258AssetAdapter<T, Currency, Amount, Moment>
where
	Currency: SetheumReservableCurrency<AccountId>,
	T: Config,
{
	fn can_reserve(who: &AccountId, value: Self::Balance) -> bool {
		Currency::can_reserve(who, value)
	}

	fn slash_reserved(who: &AccountId, value: Self::Balance) -> Self::Balance {
		let (_, gap) = Currency::slash_reserved(who, value);
		gap
	}

	fn reserved_balance(who: &AccountId) -> Self::Balance {
		Currency::reserved_balance(who)
	}

	fn reserve(who: &AccountId, value: Self::Balance) -> DispatchResult {
		Currency::reserve(who, value)
	}

	fn unreserve(who: &AccountId, value: Self::Balance) -> Self::Balance {
		Currency::unreserve(who, value)
	}

	fn repatriate_reserved(
		slashed: &AccountId,
		beneficiary: &AccountId,
		value: Self::Balance,
		status: BalanceStatus,
	) -> result::Result<Self::Balance, DispatchError> {
		Currency::repatriate_reserved(slashed, beneficiary, value, status)
	}
}

impl<T: Config> MergeAccount<T::AccountId> for Pallet<T> {
	fn merge_account(source: &T::AccountId, dest: &T::AccountId) -> DispatchResult {
		with_transaction_result(|| {
			// transfer non-native free to dest
			T::Stp258Currency::merge_account(source, dest)?;

			// unreserve all reserved currency
			T::Stp258Native::unreserve(source, T::Stp258Native::reserved_balance(source));

			// transfer all free to dest
			T::Stp258Native::transfer(source, dest, T::Stp258Native::free_balance(source))
		})
	}
}
