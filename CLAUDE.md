# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is equailizer?

A Rust CLI tool for batching, splitting, and reconciling "proxy transactions" (small loans) between two people, built on the [Lunch Money API](https://lunchmoney.dev). It manages a creditor/debtor workflow: the creditor batches tagged transactions, requests reimbursement, and then reconciles once paid — splitting settlement transactions on both sides so each person sees itemized entries in their budgeting tool. Notification emails are sent via JMAP.

## Build & Run

```bash
cargo build                  # debug build
cargo build --release        # release build
cargo run -- <command>       # run with args
cargo test                   # run all tests
cargo test <test_name>       # run a single test
```

Debug-only `dev` subcommands (email preview, transaction lookup) are available via `cargo run -- dev <subcommand>` and are gated behind `#[cfg(debug_assertions)]`.

All commands require `--profile <name>` (`-p`), which selects `profiles/<name>/config.json`.

### CLI Commands

- `create-batch` — Tags matching transactions, splits where needed, saves batch, sends notification emails
- `reconcile --batch-name <id>` — Finds settlement transactions, splits them to match batch items on both sides
- `reconcile-all` — Reconciles all unreconciled batches for a profile

### Dry Run

All commands accept `--dry-run` (`-d`) which prevents API writes and email sends. The `dry_run` flag is passed as a field on `LunchMoneyClient`, `FilePersistence`, and `JmapEmailSender`.

## Architecture

### Crate structure

The project is split into a library crate (`src/lib.rs`) and a binary crate (`src/main.rs`). The library exports all domain modules; the binary handles CLI parsing, logging, and wiring up concrete implementations.

### Trait-based dependency injection

All I/O boundaries are abstracted behind traits, enabling unit testing with mocks:

- **`LunchMoney`** (in `lunch_money/api/mod.rs`) — async trait for all Lunch Money API operations. Implemented by `LunchMoneyClient` (real HTTP) and `MockLunchMoney` (tests).
- **`Persistence`** (in `persist.rs`) — sync trait for batch storage. Implemented by `FilePersistence` (real filesystem) and `InMemoryPersistence` (tests).
- **`EmailSender`** (in `email.rs`) — async trait for sending batch notification emails. Implemented by `JmapEmailSender` (real JMAP) and `RecordingEmailSender` (tests).

Command functions (`create_batch`, `reconcile_batch`, etc.) accept `&impl Trait` parameters. `main.rs` wires up the real implementations.

### Module layout (`src/`)

- **`lib.rs`** — Library crate root, declares public modules
- **`main.rs`** — Binary entry point, CLI dispatch, constructs concrete impls
- **`cli.rs`** — `clap` derive-based CLI definitions (`Commands` enum, `StartArgs`)
- **`config.rs`** — Profile config deserialization (`Config`, `Creditor`, `Debtor`, `JMAP`); tag constants (`eq-to-split`, `eq-to-batch`)
- **`commands/`** — Business logic
  - `create_batch.rs` — Orchestrates batch creation (fetch, tag processing, updates, email)
  - `create_batch/process_tags.rs` — Filters transactions by tag, validates (no children/parent conflicts)
  - `create_batch/create_updates.rs` — Builds API update payloads for adds and splits
  - `reconcile.rs` — Orchestration + pure functions: `find_settlement_transaction`, `build_creditor_splits`, `build_debtor_splits`
- **`lunch_money/`** — Lunch Money API layer
  - `api/mod.rs` — `LunchMoney` trait, `LunchMoneyClient` struct, trait impl
  - `api/get_transactions.rs` — HTTP get functions (called by trait impl)
  - `api/update_transaction.rs` — HTTP update functions, type definitions (`TransactionUpdate`, `SplitUpdateItem`, etc.)
  - `model/transaction.rs` — `Transaction`, `Tag`, `TransactionStatus` types; `TransactionId = u32`
- **`email.rs`** — `EmailSender` trait, `JmapEmailSender`, pure HTML rendering functions, Askama templates
- **`persist.rs`** — `Persistence` trait, `FilePersistence`, `Batch`/`Settlement` types, `base_path()` utility
- **`usd.rs`** — `USD` newtype over `rust_decimal::Decimal` with arithmetic ops, serde, and random even-split logic
- **`date_helpers.rs`** — Eastern timezone date utilities
- **`log.rs`** — `tracing` setup with stdout (INFO) and rolling file (DEBUG) layers

### Data flow

1. **Create batch**: Fetch creditor transactions by date range → filter by tags (`eq-to-batch`, `eq-to-split`) → update/split via `LunchMoney` trait → save `Batch` via `Persistence` trait → send emails via `EmailSender` trait
2. **Reconcile**: Load batch via `Persistence` → fetch batch transactions → scan for settlement credit/debit using `find_settlement_transaction` → build splits using `build_creditor_splits`/`build_debtor_splits` → apply via `LunchMoney` trait → save updated batch

### Key patterns

- **Profiles**: Each profile is a directory under `profiles/` with `config.json` and a `data/` dir for batch JSON files
- **USD type**: Wraps `Decimal` with 2dp precision; custom serde rejects sub-cent precision; `random_rounded_even_split()` ensures halves sum to original
- **Templates**: Askama HTML templates in `templates/` for email bodies
- **Logging**: Rolling daily log files (`eq.log.*`) written to the project root (debug builds) or alongside the binary (release)
- **Error handling**: Uses `anyhow::Result` throughout; async traits use `async-trait` crate

### Tests

**Unit tests** (inline `#[cfg(test)]`):
- `usd.rs` — Arithmetic, serde, split behavior
- `create_batch/process_tags.rs` — Tag filtering and validation
- `create_batch/create_updates.rs` — Update payload construction

**Integration tests** (`tests/`):
- `create_batch.rs` — End-to-end batch creation with mocked API, persistence, and email
- `reconcile.rs` — Pure function tests for settlement matching and split building, plus orchestration tests
- `email.rs` — HTML template rendering verification

**Test infrastructure** (`tests/support/`):
- `builders.rs` — `test_transaction()` factory with `TransactionBuilder` trait for chainable customization
- `mocks.rs` — `MockLunchMoney`, `InMemoryPersistence`, `RecordingEmailSender`
