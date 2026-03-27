#[cfg(test)]
mod tests {
    use crate::interoperability::mocks::MockTokenClient;
    use mentorminds_escrow::{EscrowContract, EscrowContractClient, EscrowParams};
    use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env, Vec};

    #[test]
    fn test_escrow_token_transfer_chain() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let mentor = Address::generate(&env);
        let learner = Address::generate(&env);

        // 1. Deploy Mock Token (SEP-41)
        let token_id = env.register_contract(None, crate::interoperability::mocks::MockToken);
        let token_client = MockTokenClient::new(&env, &token_id);

        // 2. Deploy Escrow Contract
        let escrow_id = env.register_contract(None, EscrowContract);
        let escrow_client = EscrowContractClient::new(&env, &escrow_id);

        // 3. Initialize Escrow
        let mut approved_tokens = Vec::new(&env);
        approved_tokens.push_back(token_id.clone());
        escrow_client.initialize(&admin, &treasury, &500, &approved_tokens, &3600);

        // 4. Setup balances
        token_client.mint(&learner, &1000);
        assert_eq!(token_client.balance(&learner), 1000);

        // 5. Create Escrow (Transfer learner -> escrow)
        let params = EscrowParams {
            mentor: mentor.clone(),
            learner: learner.clone(),
            amount: 1000,
            session_id: symbol_short!("S1"),
            token_address: token_id.clone(),
            session_end_time: env.ledger().timestamp() + 3600,
            total_sessions: 1,
        };

        let est_id = escrow_client.create_escrow(&params);

        // Verify balance moved to escrow
        assert_eq!(token_client.balance(&learner), 0);
        assert_eq!(token_client.balance(&escrow_id), 1000);

        // 6. Release Funds
        escrow_client.release_funds(&learner, &est_id);

        // 7. Verify final balances (5% fee)
        assert_eq!(token_client.balance(&mentor), 950);
        assert_eq!(token_client.balance(&treasury), 50);
        assert_eq!(token_client.balance(&escrow_id), 0);
    }
}
