#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, Symbol};

#[contracttype]
#[derive(Clone, Debug)]
pub struct VerificationRecord {
    pub credential_hash: BytesN<32>,
    pub verified_at: u64,
    pub expiry: u64,
    pub is_active: bool,
}

// ---------------------------------------------------------------------------
// Storage keys
//
// Storage layout — VerificationContract
// ─────────────────────────────────────────────────────────────────────────
// All keys use `persistent()` storage so they survive ledger archival.
//
// Singleton keys:
//   DataKey::Admin                  → Address          (set once at initialize)
//
// Per-mentor keys:
//   DataKey::Verification(Address)  → VerificationRecord
//   DataKey::Tier(Address)          → i32              (reputation tier, default 0)
//
// No two keys share the same discriminant.  Because each contract has its
// own isolated storage namespace, there is no collision risk with the
// escrow or mnt-token contracts even if they use the same variant names.
// ─────────────────────────────────────────────────────────────────────────
#[contracttype]
#[derive(Clone, Debug)]
pub enum DataKey {
    /// Platform admin address.
    Admin,
    /// Verification record for a mentor, keyed by mentor address.
    Verification(Address),
    /// Reputation tier for a mentor, keyed by mentor address.
    Tier(Address),
}

#[contract]
pub struct VerificationContract;

#[contractimpl]
impl VerificationContract {
    /// Initialize the verification contract with an admin.
    /// 
    /// Auth: No authorization required for initialization.
    /// Can only be called once.
    /// 
    /// Panics if:
    /// - Contract is already initialized
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().persistent().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().persistent().set(&DataKey::Admin, &admin);
    }

    /// Verify a mentor with credentials (admin only).
    /// 
    /// Auth: Only the admin can verify mentors.
    /// The admin address is retrieved from persistent storage.
    /// 
    /// Panics if:
    /// - Contract is not initialized
    /// - Caller is not the admin
    /// - Caller fails authorization check
    pub fn verify_mentor(env: Env, mentor: Address, credential_hash: BytesN<32>, expiry: u64) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        admin.require_auth();
        let now = env.ledger().timestamp();
        let rec = VerificationRecord {
            credential_hash,
            verified_at: now,
            expiry,
            is_active: true,
        };
        let key = DataKey::Verification(mentor.clone());
        env.storage().persistent().set(&key, &rec);
        let tkey = DataKey::Tier(mentor.clone());
        if !env.storage().persistent().has(&tkey) {
            env.storage().persistent().set(&tkey, &0i32);
        }
        env.events()
            .publish((Symbol::new(&env, "mentor_vrf"), mentor), (rec.credential_hash, rec.expiry, rec.verified_at));
    }

    /// Revoke a mentor's verification (admin only).
    /// 
    /// Auth: Only the admin can revoke verifications.
    /// The admin address is retrieved from persistent storage.
    /// 
    /// Panics if:
    /// - Contract is not initialized
    /// - Caller is not the admin
    /// - Caller fails authorization check
    /// - Mentor is not verified
    pub fn revoke_verification(env: Env, mentor: Address) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        admin.require_auth();
        let key = DataKey::Verification(mentor.clone());
        let mut rec: VerificationRecord = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Not verified");
        rec.is_active = false;
        env.storage().persistent().set(&key, &rec);
        env.events()
            .publish((Symbol::new(&env, "vrf_revoked"), mentor), ());
    }

    pub fn is_verified(env: Env, mentor: Address) -> bool {
        let key = DataKey::Verification(mentor);
        let rec: Option<VerificationRecord> = env.storage().persistent().get(&key);
        match rec {
            None => false,
            Some(r) => r.is_active && env.ledger().timestamp() <= r.expiry,
        }
    }

    pub fn get_verification(env: Env, mentor: Address) -> VerificationRecord {
        let key = DataKey::Verification(mentor);
        env.storage()
            .persistent()
            .get(&key)
            .expect("Not verified")
    }
}

#[cfg(test)]
mod test {
    extern crate std;
    use super::*;
    use soroban_sdk::{testutils::{Address as AddressTestUtils, MockAuth, MockAuthInvoke, Ledger}, Address, BytesN, Env, IntoVal};

    fn setup() -> (Env, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, VerificationContract);
        let client = VerificationContractClient::new(&env, &contract_id);
        let admin = AddressTestUtils::generate(&env);
        client.initialize(&admin);
        (env, contract_id, admin)
    }

    #[test]
    fn test_verify_and_query() {
        let (env, contract_id, _admin) = setup();
        let client = VerificationContractClient::new(&env, &contract_id);
        let mentor = Address::generate(&env);
        let hash: BytesN<32> = BytesN::from_array(&env, &[0u8; 32]);
        let now = env.ledger().timestamp();
        let expiry = now + 100;
        client.verify_mentor(&mentor, &hash, &expiry);
        let rec = client.get_verification(&mentor);
        assert_eq!(rec.credential_hash, hash);
        assert_eq!(rec.is_active, true);
        assert_eq!(rec.expiry, expiry);
        assert!(rec.verified_at > 0);
        assert!(client.is_verified(&mentor));
    }

    #[test]
    fn test_admin_only_verify() {
        let (env, contract_id, _admin) = setup();
        let client = VerificationContractClient::new(&env, &contract_id);
        let mentor = Address::generate(&env);
        let hash: BytesN<32> = BytesN::from_array(&env, &[1u8; 32]);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.verify_mentor(&mentor, &hash, &0u64);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_revoke_verification() {
        let (env, contract_id, _admin) = setup();
        let client = VerificationContractClient::new(&env, &contract_id);
        let mentor = Address::generate(&env);
        let hash: BytesN<32> = BytesN::from_array(&env, &[2u8; 32]);
        let expiry = env.ledger().timestamp() + 100;
        client.verify_mentor(&mentor, &hash, &expiry);
        assert!(client.is_verified(&mentor));
        client.revoke_verification(&mentor);
        let rec = client.get_verification(&mentor);
        assert_eq!(rec.is_active, false);
        assert!(!client.is_verified(&mentor));
    }

    #[test]
    fn test_expiry_check() {
        let (env, contract_id, _admin) = setup();
        let client = VerificationContractClient::new(&env, &contract_id);
        let mentor = Address::generate(&env);
        let hash: BytesN<32> = BytesN::from_array(&env, &[3u8; 32]);
        let now = env.ledger().timestamp();
        let expiry = now + 10;
        client.verify_mentor(&mentor, &hash, &expiry);
        assert!(client.is_verified(&mentor));
        env.ledger().with_mut(|li| {
            li.timestamp += 11;
        });
        assert!(!client.is_verified(&mentor));
    }

    // -----------------------------------------------------------------------
    // Storage layout readback — full lifecycle
    // Verifies every stored value is readable after a complete lifecycle run.
    // -----------------------------------------------------------------------

    #[test]
    fn test_storage_readback_full_lifecycle() {
        let (env, contract_id, _admin) = setup();
        let client = VerificationContractClient::new(&env, &contract_id);

        let mentor = Address::generate(&env);
        let hash: BytesN<32> = BytesN::from_array(&env, &[7u8; 32]);
        let now = env.ledger().timestamp();
        let expiry = now + 500;

        // Not verified before verify_mentor
        assert!(!client.is_verified(&mentor));

        // Verify → record readable
        client.verify_mentor(&mentor, &hash, &expiry);
        let rec = client.get_verification(&mentor);
        assert_eq!(rec.credential_hash, hash);
        assert_eq!(rec.expiry, expiry);
        assert_eq!(rec.is_active, true);
        assert!(rec.verified_at > 0);
        assert!(client.is_verified(&mentor));

        // Revoke → is_active flipped, record still readable
        client.revoke_verification(&mentor);
        let rec = client.get_verification(&mentor);
        assert_eq!(rec.is_active, false);
        assert!(!client.is_verified(&mentor));

        // Re-verify with new hash → record updated
        let hash2: BytesN<32> = BytesN::from_array(&env, &[8u8; 32]);
        client.verify_mentor(&mentor, &hash2, &(expiry + 100));
        let rec2 = client.get_verification(&mentor);
        assert_eq!(rec2.credential_hash, hash2);
        assert_eq!(rec2.is_active, true);
        assert!(client.is_verified(&mentor));
    }
}