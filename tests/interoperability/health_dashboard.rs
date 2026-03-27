#[cfg(test)]
mod tests {
    use crate::interoperability::mocks::{MockHealthDashboard, MockHealthDashboardClient};
    use mentorminds_escrow::{EscrowContract, EscrowContractClient, EscrowParams};
    use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env, Vec};

    #[test]
    fn test_health_dashboard_aggregation() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let mentor = Address::generate(&env);
        let learner = Address::generate(&env);
        let treasury = Address::generate(&env);

        // 1. Deploy MNT Token (Mock)
        let token_id = env.register_contract(None, crate::interoperability::mocks::MockToken);

        // 2. Deploy Escrow
        let escrow_id = env.register_contract(None, EscrowContract);
        let escrow_client = EscrowContractClient::new(&env, &escrow_id);
        let mut approved_tokens = Vec::new(&env);
        approved_tokens.push_back(token_id.clone());
        escrow_client.initialize(&admin, &treasury, &500, &approved_tokens, &3600);

        // 3. Deploy Health Dashboard
        let dash_id = env.register_contract(None, MockHealthDashboard);
        let dash_client = MockHealthDashboardClient::new(&env, &dash_id);

        // 4. Initial check
        assert_eq!(dash_client.get_summary(&escrow_id), 0);

        // 5. Create some escrows
        let token_client = crate::interoperability::mocks::MockTokenClient::new(&env, &token_id);
        token_client.mint(&learner, &1000);
        let params = mentorminds_escrow::EscrowParams {
            mentor: mentor.clone(),
            learner: learner.clone(),
            amount: 100,
            session_id: symbol_short!("S1"),
            token_address: token_id.clone(),
            session_end_time: env.ledger().timestamp() + 3600,
            total_sessions: 1,
        };
        escrow_client.create_escrow(&params);

        let params2 = EscrowParams {
            mentor: mentor.clone(),
            learner: learner.clone(),
            amount: 200,
            session_id: symbol_short!("S2"),
            token_address: token_id.clone(),
            session_end_time: env.ledger().timestamp() + 3600,
            total_sessions: 1,
        };
        escrow_client.create_escrow(&params2);

        // 6. Verify aggregation
        assert_eq!(dash_client.get_summary(&escrow_id), 2);
    }
}
