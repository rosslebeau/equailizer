# equailizer Plugin API

Plugins are external executables that receive domain events from equailizer over a JSON Lines protocol on stdin/stdout.

## Configuration

Register plugins in your profile's `config.json`:

```json
{
  "creditor": { "..." },
  "debtor": { "..." },
  "jmap": { "..." },
  "plugins": [
    {"path": "/usr/local/bin/eq-slack-notifier", "type": "notifications"}
  ]
}
```

The `plugins` field is optional. Existing configs without it work unchanged.

### Fields

| Field  | Type   | Description |
|--------|--------|-------------|
| `path` | string | Absolute path to the plugin executable |
| `type` | string | Plugin type. Currently only `"notifications"` |

## Protocol

Communication uses JSON Lines (one JSON object per line) over the plugin's stdin (host to plugin) and stdout (plugin to host). Plugin stderr is inherited by the host process.

### Lifecycle

```
Host                          Plugin Process
  |                                |
  |-- spawn process ------------->|
  |                                |
  |-- initialize  --------------->|
  |<-- ready      ----------------|
  |                                |
  |-- batch_created  ------------>|
  |<-- ack / error  --------------|
  |                                |
  |-- shutdown  ----------------->|
  |<-- ack      ------------------|
  |                                |
  |-- (wait for exit) ----------->|
```

## Event Reference

### Host to Plugin

#### `initialize`

First message after spawn. The plugin must respond with `ready`.

```json
{
  "type": "initialize",
  "protocol_version": 1,
  "profile": "personal",
  "dry_run": false
}
```

| Field | Type | Description |
|-------|------|-------------|
| `protocol_version` | integer | Protocol version (currently `1`) |
| `profile` | string | Name of the active profile |
| `dry_run` | boolean | Whether the host is running in dry-run mode |

#### `batch_created`

A new batch was created via `create-batch`.

```json
{
  "type": "batch_created",
  "batch_id": "abc-123",
  "total": "40.00",
  "transactions": [
    {"payee": "Store A", "amount": "15.00", "date": "2025-03-01", "notes": "groceries"},
    {"payee": "Store B", "amount": "25.00", "date": "2025-03-02", "notes": null}
  ],
  "warnings": ["Transaction was tagged for batch, but it has children: 42"]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `batch_id` | string | UUID identifying the batch |
| `total` | string | Total batch amount (e.g. `"40.00"`) |
| `transactions` | array | List of transactions in the batch |
| `transactions[].payee` | string | Transaction payee name |
| `transactions[].amount` | string | Transaction amount |
| `transactions[].date` | string | Transaction date (YYYY-MM-DD) |
| `transactions[].notes` | string or null | Transaction notes |
| `warnings` | array of strings | Non-fatal issues encountered during batch creation |

#### `batch_reconciled`

A batch was successfully reconciled.

```json
{
  "type": "batch_reconciled",
  "batch_id": "abc-123",
  "amount": "40.00",
  "settlement_credit_id": 50,
  "settlement_debit_id": 60
}
```

| Field | Type | Description |
|-------|------|-------------|
| `batch_id` | string | UUID of the reconciled batch |
| `amount` | string | Batch amount |
| `settlement_credit_id` | integer | Lunch Money transaction ID for the creditor settlement |
| `settlement_debit_id` | integer | Lunch Money transaction ID for the debtor settlement |

#### `command_error`

A command failed with an error.

```json
{
  "type": "command_error",
  "command": "create-batch",
  "error": "start date cannot be after end date"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `command` | string | The command that failed (`create-batch`, `reconcile`, `reconcile-all`) |
| `error` | string | Error message |

#### `reconcile_all_complete`

The `reconcile-all` command finished (sent regardless of whether individual batches failed).

```json
{
  "type": "reconcile_all_complete",
  "reconciled_count": 3,
  "failed_count": 1,
  "errors": [
    {"batch_id": "xyz-789", "error": "settlement credit not found for batch 'xyz-789'"}
  ]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `reconciled_count` | integer | Number of batches successfully reconciled |
| `failed_count` | integer | Number of batches that failed |
| `errors` | array | Details of each failure |
| `errors[].batch_id` | string | Batch that failed |
| `errors[].error` | string | Error message |

#### `shutdown`

Plugin should clean up and prepare to exit.

```json
{"type": "shutdown"}
```

### Plugin to Host

#### `ready`

Response to `initialize`. Must be sent before the host will send events.

```json
{"type": "ready", "name": "my-plugin", "version": "1.0.0"}
```

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Plugin display name |
| `version` | string | Plugin version |

#### `ack`

Acknowledgment after receiving any event or `shutdown`.

```json
{"type": "ack"}
```

#### `error`

Report an error (non-fatal to the host). The host logs the message as a warning and continues.

```json
{"type": "error", "message": "failed to send Slack notification"}
```

## Error Handling

- If a plugin fails to start, the host logs a warning and continues without it.
- If writing to a plugin's stdin fails, the plugin is marked as dead and skipped for future events.
- If a plugin does not respond within 5 seconds, it is killed.
- If a plugin responds with `{"type": "error", ...}`, the host logs the message as a warning.
- Plugin failures never stop the host command from completing.

## Best Practices

- Respond to all messages promptly. The host has a 5-second timeout.
- Handle `shutdown` gracefully: flush buffers, close connections, then respond with `ack` and exit.
- Use stderr for logging. The host inherits plugin stderr, so log output will appear alongside host logs.
- Handle `dry_run` from the `initialize` message if your plugin has side effects (e.g., sending notifications). Consider suppressing or labeling actions during dry runs.
- Handle unknown message types gracefully (respond with `ack`) for forward compatibility.

## Rust SDK (`equailizer-plugin`)

A Rust crate is available for building plugins with typed events and minimal boilerplate. Add it as a dependency:

```toml
[dependencies]
equailizer-plugin = { git = "https://github.com/rosslebeau/equailizer", path = "equailizer-plugin" }
```

The SDK provides a `Plugin` trait with default no-op handlers and a `run()` function that manages the entire JSON Lines protocol lifecycle:

```rust
use equailizer_plugin::{Plugin, Context, BatchCreated, HandlerResult};

struct SlackNotifier;

impl Plugin for SlackNotifier {
    fn name(&self) -> &str { "slack-notifier" }
    fn version(&self) -> &str { "0.1.0" }

    fn on_batch_created(&mut self, ctx: &Context, event: &BatchCreated) -> HandlerResult {
        if ctx.dry_run { return Ok(()); }
        eprintln!("Batch {} created: {}", event.batch_id, event.total);
        Ok(())
    }
}

fn main() {
    if let Err(e) = equailizer_plugin::run(SlackNotifier) {
        eprintln!("Fatal: {e}");
        std::process::exit(1);
    }
}
```

Only `name()` and `version()` are required — all event handlers default to no-ops. Returning `Ok(())` sends `ack`; returning `Err(message)` sends an `error` response. The runner handles initialization, shutdown, and unknown message types automatically.

For testing plugins, use `equailizer_plugin::run_with_io(plugin, reader, writer)` with any `BufRead`/`Write` pair.

## Example Plugin (Python)

A minimal plugin that logs events to a file:

```python
#!/usr/bin/env python3
import json
import sys

def respond(msg):
    print(json.dumps(msg), flush=True)

def main():
    for line in sys.stdin:
        msg = json.loads(line)

        if msg["type"] == "initialize":
            respond({"type": "ready", "name": "file-logger", "version": "0.1.0"})

        elif msg["type"] == "shutdown":
            respond({"type": "ack"})
            break

        else:
            # Log the event
            with open("/tmp/eq-events.jsonl", "a") as f:
                f.write(line)
            respond({"type": "ack"})

if __name__ == "__main__":
    main()
```
