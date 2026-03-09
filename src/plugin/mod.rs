use equailizer_plugin::protocol::{
    BatchReconcileError, PluginMessage, PluginResponse, Transaction,
};

use crate::config::PluginEntry;
use crate::email::Txn;
use crate::error::Error;
use crate::persist::Batch;
use crate::usd::USD;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child, ChildStdin, ChildStdout};

use std::process::Stdio;
use std::time::Duration;

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);

struct PluginProcess {
    name: String,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    child: Child,
    dead: bool,
}

pub struct PluginManager {
    plugins: Vec<PluginProcess>,
}

// ── Domain → protocol conversion functions ──

pub fn batch_created_message(
    batch_id: &str,
    total: &USD,
    txns: &[Txn],
    warnings: &[String],
) -> PluginMessage {
    PluginMessage::BatchCreated {
        batch_id: batch_id.to_string(),
        total: total.to_string(),
        transactions: txns
            .iter()
            .map(|t| Transaction {
                payee: t.payee.clone(),
                amount: t.amount.to_string(),
                date: t.date,
                notes: t.notes.clone(),
            })
            .collect(),
        warnings: warnings.to_vec(),
    }
}

pub fn batch_reconciled_message(
    batch: &Batch,
    settlement_credit_id: u32,
    settlement_debit_id: u32,
) -> PluginMessage {
    PluginMessage::BatchReconciled {
        batch_id: batch.id.clone(),
        amount: batch.amount.to_string(),
        settlement_credit_id,
        settlement_debit_id,
    }
}

pub fn reconcile_all_complete_message(
    reconciled_count: u32,
    errors: &[Error],
) -> PluginMessage {
    PluginMessage::ReconcileAllComplete {
        reconciled_count,
        failed_count: errors.len() as u32,
        errors: errors
            .iter()
            .map(|e| match e {
                Error::BatchReconcile { batch_id, source } => BatchReconcileError {
                    batch_id: batch_id.clone(),
                    error: source.to_string(),
                },
                other => BatchReconcileError {
                    batch_id: String::new(),
                    error: other.to_string(),
                },
            })
            .collect(),
    }
}

// ── Plugin manager ──

impl PluginManager {
    /// Start all configured notification plugins. Failures during startup are
    /// logged as warnings — a failing plugin never stops the host.
    pub async fn start(entries: &[PluginEntry], profile: &str, dry_run: bool) -> Self {
        let mut plugins = Vec::new();

        for entry in entries {
            match Self::spawn_plugin(entry, profile, dry_run).await {
                Ok(process) => {
                    tracing::info!(plugin = %process.name, path = %entry.path, "Plugin started");
                    plugins.push(process);
                }
                Err(e) => {
                    tracing::warn!(path = %entry.path, error = %e, "Failed to start plugin");
                }
            }
        }

        Self { plugins }
    }

    /// Create a plugin manager with no plugins.
    pub fn empty() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    /// Send a message to all live notification plugins.
    pub async fn dispatch(&mut self, message: &PluginMessage) {
        for plugin in &mut self.plugins {
            if plugin.dead {
                continue;
            }

            if let Err(e) = Self::send_and_receive(plugin, message).await {
                tracing::warn!(
                    plugin = %plugin.name,
                    error = %e,
                    "Plugin communication failed; marking as dead"
                );
                plugin.dead = true;
            }
        }
    }

    /// Send shutdown to all live plugins and wait for them to exit.
    pub async fn shutdown(&mut self) {
        for plugin in &mut self.plugins {
            if plugin.dead {
                continue;
            }

            if let Err(e) = Self::send_and_receive(plugin, &PluginMessage::Shutdown).await {
                tracing::warn!(
                    plugin = %plugin.name,
                    error = %e,
                    "Failed to send shutdown to plugin"
                );
            }

            // Wait for the plugin to exit (stdin closes when PluginProcess is dropped).
            match tokio::time::timeout(RESPONSE_TIMEOUT, plugin.child.wait()).await {
                Ok(Ok(status)) => {
                    tracing::debug!(plugin = %plugin.name, %status, "Plugin exited");
                }
                Ok(Err(e)) => {
                    tracing::warn!(plugin = %plugin.name, error = %e, "Error waiting for plugin exit");
                }
                Err(_) => {
                    tracing::warn!(plugin = %plugin.name, "Plugin did not exit after shutdown; killing");
                    let _ = plugin.child.kill().await;
                }
            }
        }
    }

    async fn spawn_plugin(
        entry: &PluginEntry,
        profile: &str,
        dry_run: bool,
    ) -> Result<PluginProcess, String> {
        let mut child = tokio::process::Command::new(&entry.path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("failed to spawn '{}': {}", entry.path, e))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "failed to capture plugin stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "failed to capture plugin stdout".to_string())?;

        let mut process = PluginProcess {
            name: entry.path.clone(),
            stdin: BufWriter::new(stdin),
            stdout: BufReader::new(stdout),
            child,
            dead: false,
        };

        // Send initialize and wait for ready
        let init_msg = PluginMessage::Initialize {
            protocol_version: 1,
            profile: profile.to_string(),
            dry_run,
        };

        Self::write_message(&mut process, &init_msg).await?;
        let response = Self::read_response(&mut process).await?;

        match response {
            PluginResponse::Ready { name, version } => {
                tracing::info!(
                    plugin_name = %name,
                    plugin_version = %version,
                    "Plugin ready"
                );
                process.name = name;
                Ok(process)
            }
            PluginResponse::Error { message } => {
                Err(format!("plugin returned error during initialization: {message}"))
            }
            PluginResponse::Ack => {
                Err("plugin responded with ack instead of ready during initialization".to_string())
            }
        }
    }

    async fn send_and_receive(
        plugin: &mut PluginProcess,
        message: &PluginMessage,
    ) -> Result<(), String> {
        Self::write_message(plugin, message).await?;
        let response = Self::read_response(plugin).await?;

        match response {
            PluginResponse::Ack | PluginResponse::Ready { .. } => Ok(()),
            PluginResponse::Error { message } => {
                tracing::warn!(plugin = %plugin.name, error = %message, "Plugin reported error");
                Ok(())
            }
        }
    }

    async fn write_message(
        plugin: &mut PluginProcess,
        message: &PluginMessage,
    ) -> Result<(), String> {
        let mut line =
            serde_json::to_string(message).map_err(|e| format!("failed to serialize: {e}"))?;
        line.push('\n');

        plugin
            .stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| format!("failed to write to plugin stdin: {e}"))?;
        plugin
            .stdin
            .flush()
            .await
            .map_err(|e| format!("failed to flush plugin stdin: {e}"))?;

        Ok(())
    }

    async fn read_response(plugin: &mut PluginProcess) -> Result<PluginResponse, String> {
        let mut line = String::new();

        match tokio::time::timeout(RESPONSE_TIMEOUT, plugin.stdout.read_line(&mut line)).await {
            Ok(Ok(0)) => Err("plugin closed stdout (EOF)".to_string()),
            Ok(Ok(_)) => serde_json::from_str::<PluginResponse>(line.trim())
                .map_err(|e| format!("failed to parse plugin response: {e}")),
            Ok(Err(e)) => Err(format!("failed to read from plugin stdout: {e}")),
            Err(_) => {
                tracing::warn!(plugin = %plugin.name, "Plugin response timed out; killing");
                let _ = plugin.child.kill().await;
                Err("plugin response timed out".to_string())
            }
        }
    }
}
