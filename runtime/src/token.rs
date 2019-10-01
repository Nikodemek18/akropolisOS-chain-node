/// runtime module implementing the ERC20 token factory API
/// You can use mint to create tokens or burn created tokens
/// and transfer tokens on substrate side freely or operate with total_supply
///
use crate::types::{TokenBalance, TokenId};
use parity_codec::{Decode, Encode};
use rstd::prelude::Vec;
use runtime_primitives::traits::{StaticLookup, Zero};
use support::{
    decl_event, decl_module, decl_storage, dispatch::Result, ensure, StorageMap, StorageValue,
};
use system::{self, ensure_signed};

#[derive(Encode, Decode, Default, Clone, PartialEq)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct Token {
    pub id: TokenId,
    pub decimals: u16,
    pub symbol: Vec<u8>,
}

decl_event!(
    pub enum Event<T>
    where
        AccountId = <T as system::Trait>::AccountId,
    {
        NewToken(TokenId, AccountId),
        Transfer(AccountId, AccountId, TokenBalance),
        Approval(AccountId, AccountId, TokenBalance),
        Mint(AccountId, TokenBalance),
        Burn(AccountId, TokenBalance),
    }
);

pub trait Trait: balances::Trait + system::Trait {
    type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

decl_storage! {
    trait Store for Module<T: Trait> as TokenStorage {
        Count get(count): TokenId;
        Locked get(locked): map(TokenId, T::AccountId) => TokenBalance;

        TokenDefault get(token_default): Token = Token{id: 0, decimals: 18, symbol: Vec::from("TOKEN")};
        TokenIds get(token_id_by_symbol): map Vec<u8> => TokenId;
        TokenSymbol get(token_symbol_by_id): map TokenId => Vec<u8>;
        TotalSupply get(total_supply): map TokenId => TokenBalance;
        Balance get(balance_of): map (TokenId, T::AccountId) => TokenBalance;
        Allowance get(allowance_of): map (TokenId, T::AccountId, T::AccountId) => TokenBalance;
    }
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        fn deposit_event<T>() = default;

        // Burn tokens
        fn burn(origin, from: T::AccountId, #[compact] amount: TokenBalance) -> Result {
            ensure_signed(origin)?;

            // (!) : can be called directly
            // do we even need this?
            Self::_burn(0, from.clone(), amount)?;
            Self::deposit_event(RawEvent::Burn(from, amount));

            Ok(())
        }

        fn mint(origin, to: T::AccountId, #[compact] amount: TokenBalance) -> Result{
            ensure_signed(origin)?;

            let id = <TokenDefault<T>>::get().id;
            // (!) : can be called directly
            // do we even need this ?
            Self::_mint(id, to.clone(), amount)?;
            Self::deposit_event(RawEvent::Mint(to, amount));

            Ok(())
        }

        fn transfer(origin,
            to: <T::Lookup as StaticLookup>::Source,
            #[compact] amount: TokenBalance
        ) -> Result{
            let sender = ensure_signed(origin)?;
            let to = T::Lookup::lookup(to)?;
            ensure!(!amount.is_zero(), "transfer amount should be non-zero");

            let id = <TokenDefault<T>>::get().id;
            Self::make_transfer(id, sender, to, amount)?;
            Ok(())
        }

        fn approve(origin,
            spender: <T::Lookup as StaticLookup>::Source,
            #[compact] value: TokenBalance
        ) -> Result{
            let sender = ensure_signed(origin)?;
            let spender = T::Lookup::lookup(spender)?;

            let id = <TokenDefault<T>>::get().id;
            <Allowance<T>>::insert((id,sender.clone(), spender.clone()), value);

            Self::deposit_event(RawEvent::Approval(sender, spender, value));
            Ok(())
        }

        fn transfer_from(origin,
            from: T::AccountId,
            to: T::AccountId,
            #[compact] value: TokenBalance
        ) -> Result{
            let sender = ensure_signed(origin)?;
            let id = <TokenDefault<T>>::get().id;
            let allowance = Self::allowance_of((id, from.clone(), sender.clone()));

            let updated_allowance = allowance.checked_sub(value).ok_or("underflow in calculating allowance")?;


            let id = <TokenDefault<T>>::get().id;
            Self::make_transfer(id, from.clone(), to.clone(), value)?;

            <Allowance<T>>::insert((id, from, sender), updated_allowance);
            Ok(())
        }

    }
}

impl<T: Trait> Module<T> {
    pub fn _burn(token_id: TokenId, from: T::AccountId, amount: TokenBalance) -> Result {
        ensure!(
            Self::total_supply(0) >= amount,
            "Cannot burn more than total supply"
        );

        let free_balance = <Balance<T>>::get((token_id, from.clone())) - <Locked<T>>::get((token_id, from.clone()));
        ensure!(
            free_balance > TokenBalance::zero(),
            "Cannot burn with zero balance"
        );
        ensure!(free_balance >= amount, "not enough because of locked funds");

        let next_balance = free_balance
            .checked_sub(amount)
            .ok_or("underflow subtracting from balance burn")?;
        let next_total = Self::total_supply(0)
            .checked_sub(amount)
            .ok_or("underflow subtracting from total supply")?;

        <Balance<T>>::insert((token_id, from.clone()), next_balance);
        <TotalSupply<T>>::insert(token_id, next_total);

        Ok(())
    }
    pub fn _mint(token_id: TokenId, to: T::AccountId, amount: TokenBalance) -> Result {
        ensure!(!amount.is_zero(), "amount should be non-zero");

        let old_balance = <Balance<T>>::get((token_id, to.clone()));
        let next_balance = old_balance
            .checked_add(amount)
            .ok_or("overflow adding to balance")?;
        let next_total = Self::total_supply(0)
            .checked_add(amount)
            .ok_or("overflow adding to total supply")?;

        <Balance<T>>::insert((token_id, to.clone()), next_balance);
        <TotalSupply<T>>::insert(token_id, next_total);

        Ok(())
    }

    fn make_transfer(
        token_id: TokenId,
        from: T::AccountId,
        to: T::AccountId,
        amount: TokenBalance,
    ) -> Result {
        let from_balance = <Balance<T>>::get((token_id, from.clone()));
        ensure!(from_balance >= amount, "user does not have enough tokens");
        let free_balance = <Balance<T>>::get((token_id, from.clone())) - <Locked<T>>::get((token_id, from.clone()));
        ensure!(free_balance >= amount, "not enough because of locked funds");

        <Balance<T>>::insert((token_id, from.clone()), from_balance - amount);
        <Balance<T>>::mutate((token_id, to.clone()), |balance| *balance += amount);

        Self::deposit_event(RawEvent::Transfer(from, to, amount));

        Ok(())
    }
    pub fn lock(token_id: TokenId, account: T::AccountId, amount: TokenBalance) -> Result {
        <Locked<T>>::insert((token_id, account.clone()), amount);

        Ok(())
    }
    pub fn unlock(token_id: TokenId, account: &T::AccountId, amount: TokenBalance) -> Result {
        let balance = <Locked<T>>::get((token_id, account.clone()));
        let new_balance = balance
            .checked_sub(amount)
            .expect("underflow while unlocking");
        match balance - amount {
            0 => <Locked<T>>::remove((token_id, account.clone())),
            _ => <Locked<T>>::insert((token_id, account.clone()), new_balance),
        }
        Ok(())
    }
}

/// tests for this module
#[cfg(test)]
mod tests {
    use super::*;

    use primitives::{Blake2Hasher, H256};
    use runtime_io::with_externalities;
    use runtime_primitives::{
        testing::{Digest, DigestItem, Header},
        traits::{BlakeTwo256, IdentityLookup},
        BuildStorage,
    };
    use support::{assert_noop, assert_ok, impl_outer_origin};

    impl_outer_origin! {
        pub enum Origin for Test {}
    }

    // For testing the module, we construct most of a mock runtime. This means
    // first constructing a configuration type (`Test`) which `impl`s each of the
    // configuration traits of modules we want to use.
    #[derive(Clone, Eq, PartialEq)]
    pub struct Test;
    impl system::Trait for Test {
        type Origin = Origin;
        type Index = u64;
        type BlockNumber = u64;
        type Hash = H256;
        type Hashing = BlakeTwo256;
        type Digest = Digest;
        type AccountId = u64;
        type Lookup = IdentityLookup<Self::AccountId>;
        type Header = Header;
        type Event = ();
        type Log = DigestItem;
    }
    impl balances::Trait for Test {
        type Balance = u128;
        type OnFreeBalanceZero = ();
        type OnNewAccount = ();
        type TransactionPayment = ();
        type TransferPayment = ();
        type DustRemoval = ();
        type Event = ();
    }
    impl timestamp::Trait for Test {
        type Moment = u64;
        type OnTimestampSet = ();
    }
    impl Trait for Test {
        type Event = ();
    }

    type TokenModule = Module<Test>;

    // const TOKEN_NAME: &[u8; 5] = b"TOKEN";
    // const TOKEN_SHORT_NAME: &[u8; 1] = b"T";
    // const TOKEN_LONG_NAME: &[u8; 34] = b"nobody_really_want_such_long_token";
    const USER1: u64 = 1;
    const USER2: u64 = 2;

    // This function basically just builds a genesis storage key/value store according to
    // our desired mockup.
    fn new_test_ext() -> runtime_io::TestExternalities<Blake2Hasher> {
        let mut r = system::GenesisConfig::<Test>::default()
            .build_storage()
            .unwrap()
            .0;

        r.extend(
            balances::GenesisConfig::<Test> {
                balances: vec![(USER1, 100000), (USER2, 300000)],
                vesting: vec![],
                transaction_base_fee: 1,
                transaction_byte_fee: 1,
                existential_deposit: 500,
                transfer_fee: 1,
                creation_fee: 1,
            }
            .build_storage()
            .unwrap()
            .0,
        );

        r.into()
    }

    #[test]
    fn mint_new_token_works() {
        with_externalities(&mut new_test_ext(), || {
            assert_ok!(TokenModule::mint(Origin::signed(USER1), USER2, 1000));

            assert_eq!(TokenModule::balance_of((0, USER2)), 1000);
            assert_eq!(TokenModule::total_supply(0), 1000);
        })
    }

    #[test]
    fn token_transfer_works() {
        with_externalities(&mut new_test_ext(), || {
            assert_ok!(TokenModule::mint(Origin::signed(USER1), USER2, 1000));

            assert_eq!(TokenModule::balance_of((0,USER2)), 1000);
            assert_ok!(TokenModule::transfer(Origin::signed(USER2), USER1, 300));
            assert_eq!(TokenModule::balance_of((0,USER2)), 700);
            assert_eq!(TokenModule::balance_of((0,USER1)), 300);
        })
    }

    #[test]
    fn token_transfer_not_enough() {
        with_externalities(&mut new_test_ext(), || {
            assert_ok!(TokenModule::mint(Origin::signed(USER1), USER2, 1000));

            assert_eq!(TokenModule::balance_of((0,USER2)), 1000);
            assert_ok!(TokenModule::transfer(Origin::signed(USER2), USER1, 300));
            assert_eq!(TokenModule::balance_of((0,USER2)), 700);
            assert_eq!(TokenModule::balance_of((0,USER1)), 300);
            assert_eq!(TokenModule::locked((0, USER2)), 0);
            assert_noop!(
                TokenModule::transfer(Origin::signed(USER2), USER1, 1300),
                "user does not have enough tokens"
            );
        })
    }
    #[test]
    fn token_transfer_burn_works() {
        with_externalities(&mut new_test_ext(), || {
            assert_ok!(TokenModule::mint(Origin::signed(USER1), USER2, 1000));
            assert_eq!(TokenModule::balance_of((0,USER2)), 1000);

            assert_ok!(TokenModule::burn(Origin::signed(USER1), USER2, 300));
            assert_eq!(TokenModule::balance_of((0,USER2)), 700);
        })
    }
    #[test]
    fn token_transfer_burn_all_works() {
        with_externalities(&mut new_test_ext(), || {
            assert_ok!(TokenModule::mint(Origin::signed(USER1), USER2, 1000));
            assert_eq!(TokenModule::balance_of((0,USER2)), 1000);

            assert_ok!(TokenModule::burn(Origin::signed(USER1), USER2, 1000));
            assert_eq!(TokenModule::balance_of((0,USER2)), 0);
        })
    }
}
