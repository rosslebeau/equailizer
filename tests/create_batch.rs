mod support;

use equailizer::commands::create_batch::create_batch;
use equailizer::config::{Config, Creditor, Debtor, JMAP};
use equailizer::usd::USD;
use support::builders::{test_transaction, TransactionBuilder};
use support::mocks::{InMemoryPersistence, MockLunchMoney, RecordingEmailSender};

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

#[tokio::test]
async fn create_batch_with_add_tagged_transactions() {
    let config = test_config();
    let txns = vec![
        test_transaction(1, 1500)
            .with_tags(vec![("eq-to-batch", 10)])
            .with_date(2025, 3, 1)
            .with_payee("Store A"),
        test_transaction(2, 2500)
            .with_tags(vec![("eq-to-batch", 10)])
            .with_date(2025, 3, 2)
            .with_payee("Store B"),
        test_transaction(3, 999) // untagged, should be ignored
            .with_date(2025, 3, 3),
    ];

    let api = MockLunchMoney::new(txns);
    let persistence = InMemoryPersistence::new();
    let email = RecordingEmailSender::new();

    let start = chrono::NaiveDate::from_ymd_opt(2025, 3, 1).unwrap();
    let end = chrono::NaiveDate::from_ymd_opt(2025, 3, 31).unwrap();

    create_batch(start, end, &config, &api, &persistence, &email)
        .await
        .expect("create_batch should succeed");

    // Verify a batch was saved
    let batches = persistence.saved_batches();
    assert_eq!(batches.len(), 1);
    let batch = &batches[0];
    assert_eq!(batch.amount, USD::new_from_cents(4000)); // 1500 + 2500
    assert_eq!(batch.transaction_ids.len(), 2);
    assert!(batch.reconciliation.is_none());

    // Verify update calls were made (one per add-tagged txn)
    let updates = api.updates_received.lock().unwrap();
    assert_eq!(updates.len(), 2);
    // First update should be for txn 1
    assert_eq!(updates[0].0, 1);
    assert_eq!(updates[0].1.category_id, Some(99)); // proxy category
    // Second update should be for txn 2
    assert_eq!(updates[1].0, 2);

    // Verify email was sent
    assert_eq!(email.call_count(), 1);
    let calls = email.calls.lock().unwrap();
    assert_eq!(calls[0].total, USD::new_from_cents(4000));
    assert_eq!(calls[0].txn_count, 2);
}

#[tokio::test]
async fn create_batch_with_split_tagged_transactions() {
    let config = test_config();
    let txns = vec![test_transaction(10, 2000)
        .with_tags(vec![("eq-to-split", 11)])
        .with_date(2025, 4, 1)
        .with_payee("Restaurant")
        .with_category(42, "Dining")];

    let api = MockLunchMoney::new(txns)
        .with_split_ids(vec![vec![200, 201]]); // creditor split id, debtor split id

    let persistence = InMemoryPersistence::new();
    let email = RecordingEmailSender::new();

    let start = chrono::NaiveDate::from_ymd_opt(2025, 4, 1).unwrap();
    let end = chrono::NaiveDate::from_ymd_opt(2025, 4, 30).unwrap();

    create_batch(start, end, &config, &api, &persistence, &email)
        .await
        .expect("create_batch should succeed");

    // Verify batch was saved with the debtor's split ID (201)
    let batches = persistence.saved_batches();
    assert_eq!(batches.len(), 1);
    let batch = &batches[0];
    // 2000 split evenly = 1000 each, batch gets debtor's half
    assert_eq!(batch.amount, USD::new_from_cents(1000));
    assert_eq!(batch.transaction_ids, vec![201]);

    // Verify an update_and_split call was made
    let splits = api.update_and_splits_received.lock().unwrap();
    assert_eq!(splits.len(), 1);
    assert_eq!(splits[0].0, 10); // original txn id

    // Verify email was sent
    assert_eq!(email.call_count(), 1);
}

#[tokio::test]
async fn create_batch_with_no_tagged_transactions() {
    let config = test_config();
    let txns = vec![
        test_transaction(1, 1000).with_date(2025, 5, 1), // no tags
    ];

    let api = MockLunchMoney::new(txns);
    let persistence = InMemoryPersistence::new();
    let email = RecordingEmailSender::new();

    let start = chrono::NaiveDate::from_ymd_opt(2025, 5, 1).unwrap();
    let end = chrono::NaiveDate::from_ymd_opt(2025, 5, 31).unwrap();

    create_batch(start, end, &config, &api, &persistence, &email)
        .await
        .expect("create_batch should succeed");

    // No batch should be created
    assert_eq!(persistence.saved_batches().len(), 0);
    // No email should be sent
    assert_eq!(email.call_count(), 0);
}

#[tokio::test]
async fn create_batch_rejects_start_after_end() {
    let config = test_config();
    let api = MockLunchMoney::new(vec![]);
    let persistence = InMemoryPersistence::new();
    let email = RecordingEmailSender::new();

    let start = chrono::NaiveDate::from_ymd_opt(2025, 6, 30).unwrap();
    let end = chrono::NaiveDate::from_ymd_opt(2025, 6, 1).unwrap();

    let result = create_batch(start, end, &config, &api, &persistence, &email).await;

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("start date cannot be after end date")
    );
}

#[tokio::test]
async fn create_batch_skips_pending_transactions() {
    let config = test_config();
    let txns = vec![
        test_transaction(1, 1500)
            .with_tags(vec![("eq-to-batch", 10)])
            .pending(), // should be ignored
    ];

    let api = MockLunchMoney::new(txns);
    let persistence = InMemoryPersistence::new();
    let email = RecordingEmailSender::new();

    let start = chrono::NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();
    let end = chrono::NaiveDate::from_ymd_opt(2025, 7, 31).unwrap();

    create_batch(start, end, &config, &api, &persistence, &email)
        .await
        .expect("create_batch should succeed");

    // Pending txns should be filtered out, resulting in no batch
    assert_eq!(persistence.saved_batches().len(), 0);
    assert_eq!(email.call_count(), 0);
}

#[tokio::test]
async fn create_batch_issues_warning_for_add_with_children() {
    let config = test_config();
    let txns = vec![
        test_transaction(1, 1500)
            .with_tags(vec![("eq-to-batch", 10)])
            .with_date(2025, 8, 1)
            .with_payee("Valid"),
        test_transaction(2, 2000)
            .with_tags(vec![("eq-to-batch", 10)])
            .with_children(), // has children, should produce warning
    ];

    let api = MockLunchMoney::new(txns);
    let persistence = InMemoryPersistence::new();
    let email = RecordingEmailSender::new();

    let start = chrono::NaiveDate::from_ymd_opt(2025, 8, 1).unwrap();
    let end = chrono::NaiveDate::from_ymd_opt(2025, 8, 31).unwrap();

    create_batch(start, end, &config, &api, &persistence, &email)
        .await
        .expect("create_batch should succeed");

    // Only the valid txn should be in the batch
    let batches = persistence.saved_batches();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].amount, USD::new_from_cents(1500));

    // Warning should be included in email
    let calls = email.calls.lock().unwrap();
    assert_eq!(calls[0].warnings.len(), 1);
    assert!(calls[0].warnings[0].contains("has children"));
}

#[tokio::test]
async fn create_batch_mixed_add_and_split() {
    let config = test_config();
    let txns = vec![
        test_transaction(1, 3000)
            .with_tags(vec![("eq-to-batch", 10)])
            .with_date(2025, 9, 1)
            .with_payee("Full charge"),
        test_transaction(2, 2000)
            .with_tags(vec![("eq-to-split", 11)])
            .with_date(2025, 9, 2)
            .with_payee("Shared meal")
            .with_category(50, "Food"),
    ];

    let api = MockLunchMoney::new(txns)
        .with_split_ids(vec![vec![300, 301]]);
    let persistence = InMemoryPersistence::new();
    let email = RecordingEmailSender::new();

    let start = chrono::NaiveDate::from_ymd_opt(2025, 9, 1).unwrap();
    let end = chrono::NaiveDate::from_ymd_opt(2025, 9, 30).unwrap();

    create_batch(start, end, &config, &api, &persistence, &email)
        .await
        .expect("create_batch should succeed");

    let batches = persistence.saved_batches();
    assert_eq!(batches.len(), 1);
    // 3000 (full add) + 1000 (half of 2000 split) = 4000
    assert_eq!(batches[0].amount, USD::new_from_cents(4000));

    // Should have 1 update (add) + 1 update_and_split
    assert_eq!(api.updates_received.lock().unwrap().len(), 1);
    assert_eq!(api.update_and_splits_received.lock().unwrap().len(), 1);

    assert_eq!(email.call_count(), 1);
    let calls = email.calls.lock().unwrap();
    assert_eq!(calls[0].txn_count, 2);
}
