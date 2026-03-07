mod support;

use equailizer::commands::reconcile::{
    build_creditor_splits, build_debtor_splits, find_settlement_transaction,
};
use equailizer::config::{Config, Creditor, Debtor, JMAP};
use equailizer::persist::{Batch, Settlement};
use equailizer::usd::USD;
use support::builders::{test_transaction, TransactionBuilder};
use support::mocks::{InMemoryPersistence, MockLunchMoney};

fn test_config() -> Config {
    Config {
        creditor: Creditor {
            api_key: "test-creditor-key".to_string(),
            proxy_category_id: 99,
            settlement_account_id: 1000,
            email_address: "creditor@test.com".to_string(),
        },
        debtor: Debtor {
            api_key: "test-debtor-key".to_string(),
            name: "TestDebtor".to_string(),
            settlement_account_id: 2000,
            email_address: "debtor@test.com".to_string(),
            venmo_username: "testdebtor".to_string(),
        },
        jmap: JMAP {
            api_session_endpoint: "https://example.com".to_string(),
            api_key: "test-jmap-key".to_string(),
            sent_mailbox: "sent".to_string(),
            sending_address: "sender@test.com".to_string(),
        },
    }
}

// ── Pure function tests ─────────────────────────────────────────────────

#[test]
fn find_settlement_exact_match() {
    let txns = vec![
        test_transaction(1, 5000).with_account(1000),
        test_transaction(2, -3000).with_account(1000),
        test_transaction(3, -3000).with_account(9999), // wrong account
    ];

    let result = find_settlement_transaction(&txns, USD::new_from_cents(-3000), 1000);
    assert!(result.is_some());
    assert_eq!(result.unwrap().id, 2);
}

#[test]
fn find_settlement_no_match_wrong_amount() {
    let txns = vec![test_transaction(1, -5000).with_account(1000)];

    let result = find_settlement_transaction(&txns, USD::new_from_cents(-3000), 1000);
    assert!(result.is_none());
}

#[test]
fn find_settlement_no_match_wrong_account() {
    let txns = vec![test_transaction(1, -3000).with_account(9999)];

    let result = find_settlement_transaction(&txns, USD::new_from_cents(-3000), 1000);
    assert!(result.is_none());
}

#[test]
fn find_settlement_no_match_no_account() {
    // Transaction without plaid_account_id
    let txns = vec![test_transaction(1, -3000)];

    let result = find_settlement_transaction(&txns, USD::new_from_cents(-3000), 1000);
    assert!(result.is_none());
}

#[test]
fn find_settlement_empty_candidates() {
    let result = find_settlement_transaction(&[], USD::new_from_cents(-3000), 1000);
    assert!(result.is_none());
}

#[test]
fn find_settlement_returns_first_match() {
    let txns = vec![
        test_transaction(1, -3000).with_account(1000),
        test_transaction(2, -3000).with_account(1000), // also matches
    ];

    let result = find_settlement_transaction(&txns, USD::new_from_cents(-3000), 1000);
    assert_eq!(result.unwrap().id, 1);
}

#[test]
fn build_creditor_splits_correct_structure() {
    let batch_txns = vec![
        test_transaction(1, 1500)
            .with_payee("Store A")
            .with_date(2025, 3, 1),
        test_transaction(2, 2500)
            .with_payee("Store B")
            .with_date(2025, 3, 2),
    ];

    let splits = build_creditor_splits(&batch_txns, "Alice", 99);

    assert_eq!(splits.len(), 2);

    // Creditor splits have negative amounts (credits back)
    assert_eq!(splits[0].amount, USD::new_from_cents(-1500));
    assert_eq!(splits[0].payee, Some("Alice".to_string()));
    assert_eq!(splits[0].category_id, Some(99));
    assert_eq!(splits[0].notes, Some("Store A".to_string()));
    assert_eq!(
        splits[0].date,
        Some(chrono::NaiveDate::from_ymd_opt(2025, 3, 1).unwrap())
    );

    assert_eq!(splits[1].amount, USD::new_from_cents(-2500));
    assert_eq!(splits[1].payee, Some("Alice".to_string()));
    assert_eq!(splits[1].notes, Some("Store B".to_string()));
}

#[test]
fn build_debtor_splits_correct_structure() {
    let batch_txns = vec![
        test_transaction(1, 1500)
            .with_payee("Store A")
            .with_date(2025, 3, 1)
            .with_notes("groceries"),
        test_transaction(2, 2500)
            .with_payee("Store B")
            .with_date(2025, 3, 2),
    ];

    let splits = build_debtor_splits(&batch_txns);

    assert_eq!(splits.len(), 2);

    // Debtor splits have positive amounts and pass through payee/notes
    assert_eq!(splits[0].amount, USD::new_from_cents(1500));
    assert_eq!(splits[0].payee, Some("Store A".to_string()));
    assert_eq!(splits[0].category_id, None);
    assert_eq!(splits[0].notes, Some("groceries".to_string()));

    assert_eq!(splits[1].amount, USD::new_from_cents(2500));
    assert_eq!(splits[1].payee, Some("Store B".to_string()));
    assert_eq!(splits[1].notes, None);
}

#[test]
fn build_creditor_splits_empty() {
    let splits = build_creditor_splits(&[], "Alice", 99);
    assert!(splits.is_empty());
}

#[test]
fn build_debtor_splits_empty() {
    let splits = build_debtor_splits(&[]);
    assert!(splits.is_empty());
}

// ── Orchestration tests ─────────────────────────────────────────────────

#[tokio::test]
async fn reconcile_batch_end_to_end() {
    let config = test_config();

    // Batch has two transactions
    let batch_txn_1 = test_transaction(10, 1500)
        .with_payee("Store A")
        .with_date(2025, 3, 1);
    let batch_txn_2 = test_transaction(11, 2500)
        .with_payee("Store B")
        .with_date(2025, 3, 2);

    // Settlement credit on creditor side: negative batch amount (-4000) in settlement account
    let settlement_credit = test_transaction(50, -4000)
        .with_account(1000)
        .with_date(2025, 3, 5);

    // Settlement debit on debtor side: positive batch amount (4000) in settlement account
    let settlement_debit = test_transaction(60, 4000)
        .with_account(2000)
        .with_date(2025, 3, 5);

    let creditor_api =
        MockLunchMoney::new(vec![batch_txn_1, batch_txn_2, settlement_credit]);
    let debtor_api = MockLunchMoney::new(vec![settlement_debit]);

    let batch = Batch {
        id: "test-batch-1".to_string(),
        amount: USD::new_from_cents(4000),
        transaction_ids: vec![10, 11],
        reconciliation: None,
    };
    let persistence = InMemoryPersistence::with_batches(vec![batch]);

    equailizer::commands::reconcile::reconcile_batch_name(
        "test-batch-1",
        &config,
        &creditor_api,
        &debtor_api,
        &persistence,
    )
    .await
    .expect("reconcile should succeed");

    // Verify splits were applied to both sides
    let creditor_splits = creditor_api.splits_received.lock().unwrap();
    assert_eq!(creditor_splits.len(), 1);
    assert_eq!(creditor_splits[0].0, 50); // settlement credit txn id
    assert_eq!(creditor_splits[0].1.len(), 2); // two split items

    let debtor_splits = debtor_api.splits_received.lock().unwrap();
    assert_eq!(debtor_splits.len(), 1);
    assert_eq!(debtor_splits[0].0, 60); // settlement debit txn id
    assert_eq!(debtor_splits[0].1.len(), 2);

    // Verify batch was saved with settlement info
    let saved = persistence.saved_batches();
    let reconciled = saved.iter().find(|b| b.id == "test-batch-1").unwrap();
    assert!(reconciled.reconciliation.is_some());
    let settlement = reconciled.reconciliation.as_ref().unwrap();
    assert_eq!(settlement.settlement_credit_id, 50);
    assert_eq!(settlement.settlement_debit_id, 60);
}

#[tokio::test]
async fn reconcile_fails_when_no_settlement_credit() {
    let config = test_config();

    let batch_txn = test_transaction(10, 1500)
        .with_payee("Store A")
        .with_date(2025, 3, 1);

    // No matching settlement credit (wrong amount)
    let wrong_credit = test_transaction(50, -9999)
        .with_account(1000)
        .with_date(2025, 3, 5);

    let creditor_api = MockLunchMoney::new(vec![batch_txn, wrong_credit]);
    let debtor_api = MockLunchMoney::new(vec![]);

    let batch = Batch {
        id: "batch-no-credit".to_string(),
        amount: USD::new_from_cents(1500),
        transaction_ids: vec![10],
        reconciliation: None,
    };
    let persistence = InMemoryPersistence::with_batches(vec![batch]);

    let result = equailizer::commands::reconcile::reconcile_batch_name(
        "batch-no-credit",
        &config,
        &creditor_api,
        &debtor_api,
        &persistence,
    )
    .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("settlement credit"));
}

#[tokio::test]
async fn reconcile_all_processes_unreconciled_batches() {
    let config = test_config();

    let batch_txn = test_transaction(10, 1500)
        .with_payee("Store")
        .with_date(2025, 4, 1);

    let settlement_credit = test_transaction(50, -1500)
        .with_account(1000)
        .with_date(2025, 4, 5);
    let settlement_debit = test_transaction(60, 1500)
        .with_account(2000)
        .with_date(2025, 4, 5);

    let creditor_api = MockLunchMoney::new(vec![batch_txn, settlement_credit]);
    let debtor_api = MockLunchMoney::new(vec![settlement_debit]);

    let unreconciled_batch = Batch {
        id: "unreconciled-1".to_string(),
        amount: USD::new_from_cents(1500),
        transaction_ids: vec![10],
        reconciliation: None,
    };
    let already_reconciled = Batch {
        id: "already-done".to_string(),
        amount: USD::new_from_cents(500),
        transaction_ids: vec![99],
        reconciliation: Some(Settlement {
            settlement_credit_id: 200,
            settlement_debit_id: 201,
        }),
    };
    let persistence =
        InMemoryPersistence::with_batches(vec![unreconciled_batch, already_reconciled]);

    equailizer::commands::reconcile::reconcile_all(
        &config,
        &creditor_api,
        &debtor_api,
        &persistence,
    )
    .await
    .expect("reconcile_all should succeed");

    // Only the unreconciled batch should have been processed
    let splits = creditor_api.splits_received.lock().unwrap();
    assert_eq!(splits.len(), 1);

    // The unreconciled batch should now be reconciled
    let saved = persistence.saved_batches();
    let batch = saved.iter().find(|b| b.id == "unreconciled-1").unwrap();
    assert!(batch.reconciliation.is_some());
}
