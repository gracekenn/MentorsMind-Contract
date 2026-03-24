#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, String,
};
use soroban_sdk::token::TokenInterface;
use soroban_token_sdk::metadata::TokenMetadata;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InsufficientBalance = 3,
    InsufficientAllowance = 4,
    Unauthorized = 5,
    SupplyCapExceeded = 6,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Allowance(Address, Address), // (owner, spender)
    Balance(Address),
    TotalSupply,
    Metadata,
}

const SUPPLY_CAP: i128 = 100_000_000 * 10_000_000; // 100M with 7 decimals

#[contract]
pub struct MNTToken;

#[contractimpl]
impl MNTToken {
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().persistent().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().persistent().set(&DataKey::Admin, &admin);

        let metadata = TokenMetadata {
            decimal: 7,
            name: String::from_str(&env, "MentorMinds Token"),
            symbol: String::from_str(&env, "MNT"),
        };
        env.storage().persistent().set(&DataKey::Metadata, &metadata);
        env.storage().persistent().set(&DataKey::TotalSupply, &0i128);
    }

    pub fn mint(env: Env, to: Address, amount: i128) {
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).expect("Not initialized");
        admin.require_auth();

        if amount <= 0 {
            panic!("Amount must be positive");
        }

        let total_supply: i128 = env.storage().persistent().get(&DataKey::TotalSupply).unwrap_or(0);
        let new_total_supply = total_supply.checked_add(amount).expect("Overflow");
        
        if new_total_supply > SUPPLY_CAP {
            panic!("Supply cap exceeded");
        }

        let balance = Self::balance(env.clone(), to.clone());
        env.storage().persistent().set(&DataKey::Balance(to.clone()), &(balance + amount));
        env.storage().persistent().set(&DataKey::TotalSupply, &new_total_supply);

        env.events().publish(
            (symbol_short!("mint"), to),
            amount,
        );
    }

    pub fn do_burn(env: Env, from: Address, amount: i128) {
        from.require_auth();

        if amount <= 0 {
            panic!("Amount must be positive");
        }

        let balance = Self::balance(env.clone(), from.clone());
        if balance < amount {
            panic!("Insufficient balance");
        }

        let total_supply: i128 = env.storage().persistent().get(&DataKey::TotalSupply).unwrap_or(0);
        
        env.storage().persistent().set(&DataKey::Balance(from.clone()), &(balance - amount));
        env.storage().persistent().set(&DataKey::TotalSupply, &(total_supply - amount));

        env.events().publish(
            (symbol_short!("burn"), from),
            amount,
        );
    }
}

#[contractimpl]
impl TokenInterface for MNTToken {
    fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Allowance(from, spender))
            .unwrap_or(0)
    }

    fn approve(env: Env, from: Address, spender: Address, amount: i128, _expiration_ledger: u32) {
        from.require_auth();
        if amount < 0 {
            panic!("Amount must be non-negative");
        }
        env.storage()
            .persistent()
            .set(&DataKey::Allowance(from.clone(), spender.clone()), &amount);
        
        // Note: Simple implementation, expiration_ledger is usually used for TTL in Soroban
        // but for simplicity in this MVP we just store the amount.
        
        env.events().publish(
            (symbol_short!("approve"), from, spender),
            amount,
        );
    }

    fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Balance(id))
            .unwrap_or(0)
    }

    fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        if amount <= 0 {
            panic!("Amount must be positive");
        }

        let from_balance = Self::balance(env.clone(), from.clone());
        if from_balance < amount {
            panic!("Insufficient balance");
        }

        let to_balance = Self::balance(env.clone(), to.clone());

        env.storage().persistent().set(&DataKey::Balance(from.clone()), &(from_balance - amount));
        env.storage().persistent().set(&DataKey::Balance(to.clone()), &(to_balance + amount));

        env.events().publish(
            (symbol_short!("transfer"), from, to),
            amount,
        );
    }

    fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        spender.require_auth();
        if amount <= 0 {
            panic!("Amount must be positive");
        }

        let allowance = Self::allowance(env.clone(), from.clone(), spender.clone());
        if allowance < amount {
            panic!("Insufficient allowance");
        }

        let from_balance = Self::balance(env.clone(), from.clone());
        if from_balance < amount {
            panic!("Insufficient balance");
        }

        let to_balance = Self::balance(env.clone(), to.clone());

        env.storage().persistent().set(&DataKey::Allowance(from.clone(), spender.clone()), &(allowance - amount));
        env.storage().persistent().set(&DataKey::Balance(from.clone()), &(from_balance - amount));
        env.storage().persistent().set(&DataKey::Balance(to.clone()), &(to_balance + amount));

        env.events().publish(
            (symbol_short!("transfer"), from, to),
            amount,
        );
    }

    fn burn(env: Env, from: Address, amount: i128) {
        Self::do_burn(env, from, amount)
    }

    fn burn_from(env: Env, spender: Address, from: Address, amount: i128) {
        spender.require_auth();
        if amount <= 0 {
            panic!("Amount must be positive");
        }

        let allowance = Self::allowance(env.clone(), from.clone(), spender.clone());
        if allowance < amount {
            panic!("Insufficient allowance");
        }

        let from_balance = Self::balance(env.clone(), from.clone());
        if from_balance < amount {
            panic!("Insufficient balance");
        }

        let total_supply: i128 = env.storage().persistent().get(&DataKey::TotalSupply).unwrap_or(0);

        env.storage().persistent().set(&DataKey::Allowance(from.clone(), spender.clone()), &(allowance - amount));
        env.storage().persistent().set(&DataKey::Balance(from.clone()), &(from_balance - amount));
        env.storage().persistent().set(&DataKey::TotalSupply, &(total_supply - amount));

        env.events().publish(
            (symbol_short!("burn"), from),
            amount,
        );
    }

    fn decimals(_env: Env) -> u32 {
        7
    }

    fn name(env: Env) -> String {
        let metadata: TokenMetadata = env.storage().persistent().get(&DataKey::Metadata).expect("Not initialized");
        metadata.name
    }

    fn symbol(env: Env) -> String {
        let metadata: TokenMetadata = env.storage().persistent().get(&DataKey::Metadata).expect("Not initialized");
        metadata.symbol
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::{Address as _};
    use soroban_sdk::{Env};

    #[test]
    fn test_initialization() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let contract_id = env.register_contract(None, MNTToken);
        let client = MNTTokenClient::new(&env, &contract_id);

        client.initialize(&admin);

        assert_eq!(client.name(), String::from_str(&env, "MentorMinds Token"));
        assert_eq!(client.symbol(), String::from_str(&env, "MNT"));
        assert_eq!(client.decimals(), 7);
    }

    #[test]
    fn test_mint_and_burn() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let contract_id = env.register_contract(None, MNTToken);
        let client = MNTTokenClient::new(&env, &contract_id);

        client.initialize(&admin);

        client.mint(&user, &1000);
        assert_eq!(client.balance(&user), 1000);

        client.burn(&user, &400);
        assert_eq!(client.balance(&user), 600);
    }

    #[test]
    fn test_transfer_flow() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let user1 = Address::generate(&env);
        let user2 = Address::generate(&env);
        let contract_id = env.register_contract(None, MNTToken);
        let client = MNTTokenClient::new(&env, &contract_id);

        client.initialize(&admin);
        client.mint(&user1, &1000);

        client.transfer(&user1, &user2, &300);
        assert_eq!(client.balance(&user1), 700);
        assert_eq!(client.balance(&user2), 300);
    }

    #[test]
    fn test_allowance_flow() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let user1 = Address::generate(&env);
        let user2 = Address::generate(&env);
        let contract_id = env.register_contract(None, MNTToken);
        let client = MNTTokenClient::new(&env, &contract_id);

        client.initialize(&admin);
        client.mint(&user1, &1000);

        client.approve(&user1, &user2, &500, &100);
        assert_eq!(client.allowance(&user1, &user2), 500);

        client.transfer_from(&user2, &user1, &user2, &200);
        assert_eq!(client.balance(&user1), 800);
        assert_eq!(client.balance(&user2), 200);
        assert_eq!(client.allowance(&user1, &user2), 300);
    }

    #[test]
    #[should_panic(expected = "Supply cap exceeded")]
    fn test_supply_cap() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let contract_id = env.register_contract(None, MNTToken);
        let client = MNTTokenClient::new(&env, &contract_id);

        client.initialize(&admin);
        
        // Mints nearly up to cap
        client.mint(&user, &SUPPLY_CAP);
        
        // This should fail
        client.mint(&user, &1);
    }
}
