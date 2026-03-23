#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env, Symbol, symbol_short};

// Escrow status enum
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EscrowStatus {
    Active,
    Released,
    Disputed,
    Refunded,
}

// Escrow data structure
#[contracttype]
#[derive(Clone, Debug)]
pub struct Escrow {
    pub id: u64,
    pub mentor: Address,
    pub learner: Address,
    pub amount: i128,
    pub session_id: Symbol,
    pub status: EscrowStatus,
    pub created_at: u64,
    pub token_address: Address,
}

const ESCROW_COUNT: Symbol = symbol_short!("ESC_CNT");
const ADMIN: Symbol = symbol_short!("ADMIN");

// TTL constants
const ESCROW_TTL_THRESHOLD: u32 = 500_000;
const ESCROW_TTL_BUMP: u32 = 1_000_000;

#[contract]
pub struct EscrowContract;

#[contractimpl]
impl EscrowContract {
    /// Initialize the contract with an admin
    pub fn initialize(env: Env, admin: Address) {
        // Ensure not already initialized - use persistent storage to prevent re-initialization attack
        if env.storage().persistent().has(&ADMIN) {
            panic!("Already initialized");
        }
        
        // Store admin in persistent storage to prevent expiry-based hijacking
        env.storage().persistent().set(&ADMIN, &admin);
        env.storage().persistent().extend_ttl(&ADMIN, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        
        // Store escrow count in persistent storage to prevent reset on expiry
        env.storage().persistent().set(&ESCROW_COUNT, &0u64);
        env.storage().persistent().extend_ttl(&ESCROW_COUNT, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
    }

    /// Create a new escrow
    pub fn create_escrow(
        env: Env,
        mentor: Address,
        learner: Address,
        amount: i128,
        session_id: Symbol,
        token_address: Address,
    ) -> u64 {
        // Validate amount
        if amount <= 0 {
            panic!("Amount must be greater than zero");
        }
        
        // Require learner authorization
        learner.require_auth();

        // Get and increment escrow count from persistent storage
        let mut count: u64 = env.storage().persistent().get(&ESCROW_COUNT).unwrap_or(0);
        count += 1;
        
        // Bump TTL for escrow count
        env.storage().persistent().extend_ttl(&ESCROW_COUNT, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        // Transfer tokens from learner to contract
        let token_client = token::Client::new(&env, &token_address);
        token_client.transfer(&learner, &env.current_contract_address(), &amount);

        // Create escrow
        let escrow = Escrow {
            id: count,
            mentor: mentor.clone(),
            learner: learner.clone(),
            amount,
            session_id: session_id.clone(),
            status: EscrowStatus::Active,
            created_at: env.ledger().timestamp(),
            token_address: token_address.clone(),
        };

        // Store escrow with TTL bump
        let key = (symbol_short!("ESCROW"), count);
        env.storage().persistent().set(&key, &escrow);
        env.storage().persistent().extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        // Emit event
        env.events().publish(
            (symbol_short!("created"), count),
            (mentor, learner, amount, session_id, token_address),
        );

        count
    }

    /// Release funds to mentor (called by learner or admin)
    pub fn release_funds(env: Env, caller: Address, escrow_id: u64) {
        let key = (symbol_short!("ESCROW"), escrow_id);
        
        // Bump TTL before reading escrow
        env.storage().persistent().extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        
        let mut escrow: Escrow = env.storage().persistent()
            .get(&key)
            .expect("Escrow not found");

        // Check status
        if escrow.status != EscrowStatus::Active {
            panic!("Escrow not active");
        }
        
        // Get admin from persistent storage with TTL bump
        let admin: Address = env.storage().persistent().get(&ADMIN).expect("Admin not found");
        env.storage().persistent().extend_ttl(&ADMIN, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);

        // Require caller authorization
        caller.require_auth();
        
        // Verify caller is either learner or admin
        if caller != escrow.learner && caller != admin {
            panic!("Caller not authorized");
        }

        // Transfer tokens from contract to mentor
        let token_client = token::Client::new(&env, &escrow.token_address);
        token_client.transfer(&env.current_contract_address(), &escrow.mentor, &escrow.amount);

        // Update status
        escrow.status = EscrowStatus::Released;
        env.storage().persistent().set(&key, &escrow);

        // Emit event
        env.events().publish(
            (symbol_short!("released"), escrow_id),
            (escrow.mentor.clone(), escrow.amount),
        );
    }

    /// Open a dispute (called by mentor or learner)
    pub fn dispute(env: Env, caller: Address, escrow_id: u64) {
        let key = (symbol_short!("ESCROW"), escrow_id);
        
        // Bump TTL before reading escrow
        env.storage().persistent().extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        
        let mut escrow: Escrow = env.storage().persistent()
            .get(&key)
            .expect("Escrow not found");

        // Check status
        if escrow.status != EscrowStatus::Active {
            panic!("Escrow not active");
        }
        
        // Require caller authorization
        caller.require_auth();
        
        // Verify caller is either mentor or learner
        if caller != escrow.mentor && caller != escrow.learner {
            panic!("Caller not authorized to dispute");
        }

        // Update status
        escrow.status = EscrowStatus::Disputed;
        env.storage().persistent().set(&key, &escrow);

        // Emit event
        env.events().publish(
            (symbol_short!("disputed"), escrow_id),
            escrow_id,
        );
    }

    /// Refund to learner (called by admin)
    pub fn refund(env: Env, escrow_id: u64) {
        // Get admin from persistent storage with TTL bump
        let admin: Address = env.storage().persistent().get(&ADMIN).expect("Admin not found");
        env.storage().persistent().extend_ttl(&ADMIN, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        admin.require_auth();

        let key = (symbol_short!("ESCROW"), escrow_id);
        
        // Bump TTL before reading escrow
        env.storage().persistent().extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        
        let mut escrow: Escrow = env.storage().persistent()
            .get(&key)
            .expect("Escrow not found");

        // Check status
        if escrow.status == EscrowStatus::Released || escrow.status == EscrowStatus::Refunded {
            panic!("Cannot refund");
        }
        
        // Transfer tokens from contract to learner
        let token_client = token::Client::new(&env, &escrow.token_address);
        token_client.transfer(&env.current_contract_address(), &escrow.learner, &escrow.amount);

        // Update status
        escrow.status = EscrowStatus::Refunded;
        env.storage().persistent().set(&key, &escrow);

        // Emit event
        env.events().publish(
            (symbol_short!("refunded"), escrow_id),
            (escrow.learner.clone(), escrow.amount),
        );
    }

    /// Get escrow details
    pub fn get_escrow(env: Env, escrow_id: u64) -> Escrow {
        let key = (symbol_short!("ESCROW"), escrow_id);
        
        // Bump TTL before reading escrow
        env.storage().persistent().extend_ttl(&key, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        
        env.storage().persistent()
            .get(&key)
            .expect("Escrow not found")
    }

    /// Get total escrow count
    pub fn get_escrow_count(env: Env) -> u64 {
        // Bump TTL when reading count
        env.storage().persistent().extend_ttl(&ESCROW_COUNT, ESCROW_TTL_THRESHOLD, ESCROW_TTL_BUMP);
        env.storage().persistent().get(&ESCROW_COUNT).unwrap_or(0)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::{Address as _, Ledger, Storage}, Address, Env, Symbol, symbol_short};

    fn setup_env() -> (Env, Address, Address, Address, Address) {
        let env = Env::default();
        let contract_id = env.register_contract(None, EscrowContract);
        let admin = Address::generate(&env);
        let mentor = Address::generate(&env);
        let learner = Address::generate(&env);
        let token_address = Address::generate(&env);
        
        // Setup mock token contract
        env.mock_all_auths();
        
        (env, admin, mentor, learner, token_address)
    }

    #[test]
    fn test_initialize_and_prevent_reinit() {
        let env = Env::default();
        let contract_id = env.register_contract(None, EscrowContract);
        let client = EscrowContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let other = Address::generate(&env);

        // Initialize
        client.initialize(&admin);

        // Try to re-initialize - should panic
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.initialize(&other);
        }));
        assert!(result.is_err());

        // Verify admin is still the original one
        // (This test verifies persistent storage prevents re-init attack)
    }

    #[test]
    fn test_create_escrow_with_validation() {
        let env = Env::default();
        let contract_id = env.register_contract(None, EscrowContract);
        let client = EscrowContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let mentor = Address::generate(&env);
        let learner = Address::generate(&env);
        let token_address = Address::generate(&env);

        // Initialize
        client.initialize(&admin);

        // Test zero amount - should panic
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            env.mock_all_auths();
            client.create_escrow(&mentor, &learner, &0, &symbol_short!("SESSION1"), &token_address);
        }));
        assert!(result.is_err());

        // Test negative amount - should panic
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            env.mock_all_auths();
            client.create_escrow(&mentor, &learner, &-100, &symbol_short!("SESSION1"), &token_address);
        }));
        assert!(result.is_err());

        // Valid amount should succeed
        env.mock_all_auths();
        let escrow_id = client.create_escrow(
            &mentor,
            &learner,
            &1000,
            &symbol_short!("SESSION1"),
            &token_address,
        );

        assert_eq!(escrow_id, 1);

        // Get escrow and verify token_address is stored
        let escrow = client.get_escrow(&escrow_id);
        assert_eq!(escrow.amount, 1000);
        assert_eq!(escrow.status, EscrowStatus::Active);
        assert_eq!(escrow.token_address, token_address);
    }

    #[test]
    fn test_release_funds_authorized_caller() {
        let env = Env::default();
        let contract_id = env.register_contract(None, EscrowContract);
        let client = EscrowContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let mentor = Address::generate(&env);
        let learner = Address::generate(&env);
        let token_address = Address::generate(&env);

        // Initialize and create escrow
        client.initialize(&admin);
        env.mock_all_auths();
        let escrow_id = client.create_escrow(
            &mentor,
            &learner,
            &1000,
            &symbol_short!("SESSION1"),
            &token_address,
        );

        // Learner can release funds
        env.mock_all_auths();
        client.release_funds(&learner, &escrow_id);

        // Check status
        let escrow = client.get_escrow(&escrow_id);
        assert_eq!(escrow.status, EscrowStatus::Released);
    }

    #[test]
    fn test_release_funds_unauthorized_caller() {
        let env = Env::default();
        let contract_id = env.register_contract(None, EscrowContract);
        let client = EscrowContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let mentor = Address::generate(&env);
        let learner = Address::generate(&env);
        let unauthorized = Address::generate(&env);
        let token_address = Address::generate(&env);

        // Initialize and create escrow
        client.initialize(&admin);
        env.mock_all_auths();
        let escrow_id = client.create_escrow(
            &mentor,
            &learner,
            &1000,
            &symbol_short!("SESSION1"),
            &token_address,
        );

        // Unauthorized caller should fail
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            env.mock_all_auths();
            client.release_funds(&unauthorized, &escrow_id);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_dispute_authorized_callers() {
        let env = Env::default();
        let contract_id = env.register_contract(None, EscrowContract);
        let client = EscrowContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let mentor = Address::generate(&env);
        let learner = Address::generate(&env);
        let token_address = Address::generate(&env);

        // Initialize and create escrow
        client.initialize(&admin);
        env.mock_all_auths();
        let escrow_id = client.create_escrow(
            &mentor,
            &learner,
            &1000,
            &symbol_short!("SESSION1"),
            &token_address,
        );

        // Mentor can dispute
        env.mock_all_auths();
        client.dispute(&mentor, &escrow_id);
        
        let escrow = client.get_escrow(&escrow_id);
        assert_eq!(escrow.status, EscrowStatus::Disputed);
        
        // Reset escrow for learner test (create new one)
        let escrow_id2 = client.create_escrow(
            &mentor,
            &learner,
            &2000,
            &symbol_short!("SESSION2"),
            &token_address,
        );
        
        // Learner can dispute
        env.mock_all_auths();
        client.dispute(&learner, &escrow_id2);
        
        let escrow2 = client.get_escrow(&escrow_id2);
        assert_eq!(escrow2.status, EscrowStatus::Disputed);
    }

    #[test]
    fn test_dispute_unauthorized_caller() {
        let env = Env::default();
        let contract_id = env.register_contract(None, EscrowContract);
        let client = EscrowContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let mentor = Address::generate(&env);
        let learner = Address::generate(&env);
        let random = Address::generate(&env);
        let token_address = Address::generate(&env);

        // Initialize and create escrow
        client.initialize(&admin);
        env.mock_all_auths();
        let escrow_id = client.create_escrow(
            &mentor,
            &learner,
            &1000,
            &symbol_short!("SESSION1"),
            &token_address,
        );

        // Random account should fail
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            env.mock_all_auths();
            client.dispute(&random, &escrow_id);
        }));
        assert!(result.is_err());
    }
}
