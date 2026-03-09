use std::io::{BufRead, Write};

use crate::context::{Context, Error, HandlerResult};
use crate::events::{BatchCreated, BatchReconciled, CommandError, ReconcileAllComplete};
use crate::plugin::Plugin;
use crate::protocol::{PluginMessage, PluginResponse};

/// Run the plugin event loop, reading from stdin and writing to stdout.
///
/// Returns `Ok(())` on shutdown or stdin EOF.
pub fn run(plugin: impl Plugin) -> Result<(), Error> {
    run_with_io(plugin, std::io::stdin().lock(), std::io::stdout().lock())
}

/// Run the plugin event loop with explicit I/O sources. Useful for testing.
pub fn run_with_io(
    mut plugin: impl Plugin,
    mut reader: impl BufRead,
    mut writer: impl Write,
) -> Result<(), Error> {
    let mut line = String::new();

    // Read and validate the initialize message.
    let bytes = reader.read_line(&mut line).map_err(Error::Io)?;
    if bytes == 0 {
        return Err(Error::Protocol(
            "unexpected EOF before initialize".to_string(),
        ));
    }

    let value: serde_json::Value = serde_json::from_str(line.trim())
        .map_err(|e| Error::Protocol(format!("invalid JSON: {e}")))?;
    let msg: PluginMessage = serde_json::from_value(value)
        .map_err(|e| Error::Protocol(format!("failed to parse initialize: {e}")))?;

    let ctx = match msg {
        PluginMessage::Initialize {
            protocol_version,
            profile,
            dry_run,
        } => Context {
            protocol_version,
            profile,
            dry_run,
        },
        _ => {
            return Err(Error::Protocol(
                "first message must be initialize".to_string(),
            ))
        }
    };

    // Send ready response.
    write_response(
        &mut writer,
        &PluginResponse::Ready {
            name: plugin.name().to_string(),
            version: plugin.version().to_string(),
        },
    )?;

    // Event loop.
    loop {
        line.clear();
        let bytes = reader.read_line(&mut line).map_err(Error::Io)?;
        if bytes == 0 {
            return Ok(()); // EOF
        }

        // Parse as JSON first, then try to match a known message type.
        let value: serde_json::Value = serde_json::from_str(line.trim())
            .map_err(|e| Error::Protocol(format!("invalid JSON: {e}")))?;

        match serde_json::from_value::<PluginMessage>(value) {
            Ok(PluginMessage::Shutdown) => {
                let response = to_response(plugin.on_shutdown());
                write_response(&mut writer, &response)?;
                return Ok(());
            }
            Ok(msg) => {
                let response = dispatch_event(&mut plugin, &ctx, msg);
                write_response(&mut writer, &response)?;
            }
            Err(_) => {
                // Unknown message type — forward compatibility, send ack.
                write_response(&mut writer, &PluginResponse::Ack)?;
            }
        }
    }
}

fn dispatch_event(
    plugin: &mut impl Plugin,
    ctx: &Context,
    msg: PluginMessage,
) -> PluginResponse {
    let handler_result = match msg {
        PluginMessage::BatchCreated {
            batch_id,
            total,
            transactions,
            warnings,
        } => plugin.on_batch_created(
            ctx,
            &BatchCreated {
                batch_id,
                total,
                transactions,
                warnings,
            },
        ),
        PluginMessage::BatchReconciled {
            batch_id,
            amount,
            settlement_credit_id,
            settlement_debit_id,
        } => plugin.on_batch_reconciled(
            ctx,
            &BatchReconciled {
                batch_id,
                amount,
                settlement_credit_id,
                settlement_debit_id,
            },
        ),
        PluginMessage::CommandError { command, error } => {
            plugin.on_command_error(ctx, &CommandError { command, error })
        }
        PluginMessage::ReconcileAllComplete {
            reconciled_count,
            failed_count,
            errors,
        } => plugin.on_reconcile_all_complete(
            ctx,
            &ReconcileAllComplete {
                reconciled_count,
                failed_count,
                errors,
            },
        ),
        // Initialize and Shutdown are handled before reaching this function.
        PluginMessage::Initialize { .. } | PluginMessage::Shutdown => return PluginResponse::Ack,
    };
    to_response(handler_result)
}

fn to_response(result: HandlerResult) -> PluginResponse {
    match result {
        Ok(()) => PluginResponse::Ack,
        Err(message) => PluginResponse::Error { message },
    }
}

fn write_response(writer: &mut impl Write, response: &PluginResponse) -> Result<(), Error> {
    let mut json = serde_json::to_string(response)
        .map_err(|e| Error::Protocol(format!("failed to serialize response: {e}")))?;
    json.push('\n');
    writer.write_all(json.as_bytes()).map_err(Error::Io)?;
    writer.flush().map_err(Error::Io)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Plugin;

    struct MinimalPlugin;

    impl Plugin for MinimalPlugin {
        fn name(&self) -> &str {
            "test-plugin"
        }
        fn version(&self) -> &str {
            "0.1.0"
        }
    }

    fn parse_output_lines(output: &[u8]) -> Vec<serde_json::Value> {
        std::str::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[test]
    fn full_lifecycle() {
        let input = [
            r#"{"type":"initialize","protocol_version":1,"profile":"test","dry_run":false}"#,
            r#"{"type":"batch_created","batch_id":"abc","total":"10.00","transactions":[],"warnings":[]}"#,
            r#"{"type":"shutdown"}"#,
        ]
        .join("\n")
            + "\n";

        let mut output = Vec::new();
        let result = run_with_io(MinimalPlugin, input.as_bytes(), &mut output);
        assert!(result.is_ok());

        let lines = parse_output_lines(&output);
        assert_eq!(lines.len(), 3); // ready, ack, ack (shutdown)

        assert_eq!(lines[0]["type"], "ready");
        assert_eq!(lines[0]["name"], "test-plugin");
        assert_eq!(lines[0]["version"], "0.1.0");
        assert_eq!(lines[1]["type"], "ack");
        assert_eq!(lines[2]["type"], "ack");
    }

    #[test]
    fn eof_after_initialize() {
        let input = r#"{"type":"initialize","protocol_version":1,"profile":"test","dry_run":false}"#.to_string() + "\n";

        let mut output = Vec::new();
        let result = run_with_io(MinimalPlugin, input.as_bytes(), &mut output);
        assert!(result.is_ok());

        let lines = parse_output_lines(&output);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["type"], "ready");
    }

    #[test]
    fn unknown_message_type_sends_ack() {
        let input = [
            r#"{"type":"initialize","protocol_version":1,"profile":"test","dry_run":false}"#,
            r#"{"type":"future_event","data":"something"}"#,
            r#"{"type":"shutdown"}"#,
        ]
        .join("\n")
            + "\n";

        let mut output = Vec::new();
        let result = run_with_io(MinimalPlugin, input.as_bytes(), &mut output);
        assert!(result.is_ok());

        let lines = parse_output_lines(&output);
        assert_eq!(lines.len(), 3); // ready, ack (unknown), ack (shutdown)
        assert_eq!(lines[1]["type"], "ack");
    }

    #[test]
    fn handler_error_sends_error_response() {
        struct FailingPlugin;

        impl Plugin for FailingPlugin {
            fn name(&self) -> &str {
                "failing"
            }
            fn version(&self) -> &str {
                "0.1.0"
            }
            fn on_batch_created(
                &mut self,
                _ctx: &Context,
                _event: &BatchCreated,
            ) -> HandlerResult {
                Err("slack notification failed".to_string())
            }
        }

        let input = [
            r#"{"type":"initialize","protocol_version":1,"profile":"test","dry_run":false}"#,
            r#"{"type":"batch_created","batch_id":"abc","total":"10.00","transactions":[],"warnings":[]}"#,
            r#"{"type":"shutdown"}"#,
        ]
        .join("\n")
            + "\n";

        let mut output = Vec::new();
        let result = run_with_io(FailingPlugin, input.as_bytes(), &mut output);
        assert!(result.is_ok());

        let lines = parse_output_lines(&output);
        assert_eq!(lines[1]["type"], "error");
        assert_eq!(lines[1]["message"], "slack notification failed");
    }

    #[test]
    fn context_passed_to_handlers() {
        use std::sync::{Arc, Mutex};

        struct CapturingPlugin {
            captured_profile: Arc<Mutex<String>>,
            captured_dry_run: Arc<Mutex<bool>>,
        }

        impl Plugin for CapturingPlugin {
            fn name(&self) -> &str {
                "capturing"
            }
            fn version(&self) -> &str {
                "0.1.0"
            }
            fn on_batch_created(
                &mut self,
                ctx: &Context,
                _event: &BatchCreated,
            ) -> HandlerResult {
                *self.captured_profile.lock().unwrap() = ctx.profile.clone();
                *self.captured_dry_run.lock().unwrap() = ctx.dry_run;
                Ok(())
            }
        }

        let profile = Arc::new(Mutex::new(String::new()));
        let dry_run = Arc::new(Mutex::new(false));

        let plugin = CapturingPlugin {
            captured_profile: Arc::clone(&profile),
            captured_dry_run: Arc::clone(&dry_run),
        };

        let input = [
            r#"{"type":"initialize","protocol_version":1,"profile":"personal","dry_run":true}"#,
            r#"{"type":"batch_created","batch_id":"abc","total":"10.00","transactions":[],"warnings":[]}"#,
            r#"{"type":"shutdown"}"#,
        ]
        .join("\n")
            + "\n";

        let mut output = Vec::new();
        run_with_io(plugin, input.as_bytes(), &mut output).unwrap();

        assert_eq!(*profile.lock().unwrap(), "personal");
        assert!(*dry_run.lock().unwrap());
    }

    #[test]
    fn all_event_types_dispatched() {
        use std::sync::{Arc, Mutex};

        #[derive(Default)]
        struct Tracker {
            batch_created: bool,
            batch_reconciled: bool,
            command_error: bool,
            reconcile_all_complete: bool,
            shutdown: bool,
        }

        struct TrackingPlugin {
            tracker: Arc<Mutex<Tracker>>,
        }

        impl Plugin for TrackingPlugin {
            fn name(&self) -> &str {
                "tracker"
            }
            fn version(&self) -> &str {
                "0.1.0"
            }
            fn on_batch_created(
                &mut self,
                _ctx: &Context,
                _event: &BatchCreated,
            ) -> HandlerResult {
                self.tracker.lock().unwrap().batch_created = true;
                Ok(())
            }
            fn on_batch_reconciled(
                &mut self,
                _ctx: &Context,
                _event: &crate::events::BatchReconciled,
            ) -> HandlerResult {
                self.tracker.lock().unwrap().batch_reconciled = true;
                Ok(())
            }
            fn on_command_error(
                &mut self,
                _ctx: &Context,
                _event: &crate::events::CommandError,
            ) -> HandlerResult {
                self.tracker.lock().unwrap().command_error = true;
                Ok(())
            }
            fn on_reconcile_all_complete(
                &mut self,
                _ctx: &Context,
                _event: &crate::events::ReconcileAllComplete,
            ) -> HandlerResult {
                self.tracker.lock().unwrap().reconcile_all_complete = true;
                Ok(())
            }
            fn on_shutdown(&mut self) -> HandlerResult {
                self.tracker.lock().unwrap().shutdown = true;
                Ok(())
            }
        }

        let tracker = Arc::new(Mutex::new(Tracker::default()));
        let plugin = TrackingPlugin {
            tracker: Arc::clone(&tracker),
        };

        let input = [
            r#"{"type":"initialize","protocol_version":1,"profile":"test","dry_run":false}"#,
            r#"{"type":"batch_created","batch_id":"a","total":"1.00","transactions":[],"warnings":[]}"#,
            r#"{"type":"batch_reconciled","batch_id":"a","amount":"1.00","settlement_credit_id":1,"settlement_debit_id":2}"#,
            r#"{"type":"command_error","command":"create-batch","error":"oops"}"#,
            r#"{"type":"reconcile_all_complete","reconciled_count":1,"failed_count":0,"errors":[]}"#,
            r#"{"type":"shutdown"}"#,
        ]
        .join("\n")
            + "\n";

        let mut output = Vec::new();
        run_with_io(plugin, input.as_bytes(), &mut output).unwrap();

        let t = tracker.lock().unwrap();
        assert!(t.batch_created);
        assert!(t.batch_reconciled);
        assert!(t.command_error);
        assert!(t.reconcile_all_complete);
        assert!(t.shutdown);
    }

    #[test]
    fn eof_before_any_input_is_error() {
        let mut output = Vec::new();
        let result = run_with_io(MinimalPlugin, &[][..], &mut output);
        assert!(result.is_err());
    }

    #[test]
    fn non_initialize_first_message_is_error() {
        let input = r#"{"type":"shutdown"}"#.to_string() + "\n";

        let mut output = Vec::new();
        let result = run_with_io(MinimalPlugin, input.as_bytes(), &mut output);
        assert!(result.is_err());
    }
}
