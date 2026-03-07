use equailizer::email::{make_creditor_email_html_string, make_debtor_email_html_string, Txn};
use equailizer::usd::USD;

#[test]
fn creditor_email_html_contains_key_elements() {
    let txns = vec![
        Txn {
            payee: "Store A".to_string(),
            amount: USD::new_from_cents(1500),
            date: chrono::NaiveDate::from_ymd_opt(2025, 3, 1).unwrap(),
            notes: Some("groceries".to_string()),
        },
        Txn {
            payee: "Store B".to_string(),
            amount: USD::new_from_cents(2500),
            date: chrono::NaiveDate::from_ymd_opt(2025, 3, 2).unwrap(),
            notes: None,
        },
    ];
    let venmo_link = "https://venmo.com/test?txn=charge&amount=40.00".to_string();
    let batch_id = "test-batch-123".to_string();
    let total = USD::new_from_cents(4000);

    let html = make_creditor_email_html_string(
        &txns,
        &venmo_link,
        vec!["Warning: something happened".to_string()],
        &batch_id,
        &total,
    );

    assert!(html.contains("Store A"));
    assert!(html.contains("Store B"));
    assert!(html.contains("15.00"));
    assert!(html.contains("25.00"));
    assert!(html.contains("40.00"));
    // Askama HTML-escapes the & in URLs
    assert!(html.contains("venmo.com/test"));
    assert!(html.contains("test-batch-123"));
    assert!(html.contains("Warning: something happened"));
    assert!(html.contains("groceries"));
}

#[test]
fn creditor_email_html_no_warnings() {
    let txns = vec![Txn {
        payee: "Store".to_string(),
        amount: USD::new_from_cents(1000),
        date: chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
        notes: None,
    }];
    let venmo_link = "https://venmo.com/test".to_string();
    let batch_id = "batch-1".to_string();
    let total = USD::new_from_cents(1000);

    let html = make_creditor_email_html_string(&txns, &venmo_link, vec![], &batch_id, &total);

    assert!(html.contains("Store"));
    assert!(html.contains("10.00"));
}

#[test]
fn debtor_email_html_contains_key_elements() {
    let txns = vec![
        Txn {
            payee: "Store A".to_string(),
            amount: USD::new_from_cents(1500),
            date: chrono::NaiveDate::from_ymd_opt(2025, 3, 1).unwrap(),
            notes: Some("weekly groceries".to_string()),
        },
        Txn {
            payee: "Store B".to_string(),
            amount: USD::new_from_cents(2500),
            date: chrono::NaiveDate::from_ymd_opt(2025, 3, 2).unwrap(),
            notes: None,
        },
    ];
    let batch_id = "test-batch-456".to_string();
    let total = USD::new_from_cents(4000);

    let html = make_debtor_email_html_string(&txns, &batch_id, &total);

    assert!(html.contains("Store A"));
    assert!(html.contains("Store B"));
    assert!(html.contains("15.00"));
    assert!(html.contains("25.00"));
    assert!(html.contains("40.00"));
    assert!(html.contains("test-batch-456"));
    assert!(html.contains("weekly groceries"));
}

#[test]
fn debtor_email_html_single_transaction() {
    let txns = vec![Txn {
        payee: "Single Store".to_string(),
        amount: USD::new_from_cents(999),
        date: chrono::NaiveDate::from_ymd_opt(2025, 6, 15).unwrap(),
        notes: None,
    }];
    let batch_id = "single-batch".to_string();
    let total = USD::new_from_cents(999);

    let html = make_debtor_email_html_string(&txns, &batch_id, &total);

    assert!(html.contains("Single Store"));
    assert!(html.contains("9.99"));
}
