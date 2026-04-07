pub mod pop;
pub mod vending;

use crate::Data;
use crate::db::models::ServerSettings;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Context passed to every in-game command.
pub struct GContext<'a> {
    pub server_id: i32,
    pub ip: String,
    pub port: i32,
    pub settings: ServerSettings,
    pub data: &'a Data,
}

/// The trait that all in-game commands must implement.
/// We use Pin<Box<dyn Future>> because async fn in traits is not dyn-compatible yet.
pub trait GCommand: Send + Sync {
    /// The name of the command (e.g. "pop").
    fn name(&self) -> &'static str;

    /// Logic to execute when the command is called.
    fn execute<'a>(
        &'a self,
        ctx: GContext<'a>,
        args: &'a [&'a str],
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Option<String>>> + Send + 'a>>;
}

/// Registry that manages all registered in-game commands.
pub struct GCommandRegistry {
    commands: HashMap<String, Arc<dyn GCommand>>,
}

impl GCommandRegistry {
    #[must_use]
    pub fn new() -> Self {
        let mut registry = Self {
            commands: HashMap::new(),
        };

        // Register commands here
        registry.register(pop::Pop);
        registry.register(vending::Vending);

        registry
    }

    fn register<T: GCommand + 'static>(&mut self, command: T) {
        self.commands
            .insert(command.name().to_lowercase(), Arc::new(command));
    }

    /// Handles an incoming team chat message, checking for prefix and executing commands.
    ///
    /// # Errors
    /// Returns an error if the command execution fails.
    pub async fn handle_message(
        &self,
        message: &str,
        server_id: i32,
        ip: &str,
        port: i32,
        settings: &ServerSettings,
        data: &Data,
    ) -> anyhow::Result<Option<String>> {
        let prefix = &settings.in_game_prefix;
        if !message.starts_with(prefix) {
            return Ok(None);
        }

        let cmd_body = message[prefix.len()..].trim();
        let parts: Vec<&str> = cmd_body.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(None);
        }

        let cmd_name = parts[0].to_lowercase();
        let args = &parts[1..];

        if let Some(command) = self.commands.get(&cmd_name) {
            let ctx = GContext {
                server_id,
                ip: ip.to_string(),
                port,
                settings: settings.clone(),
                data,
            };
            command.execute(ctx, args).await
        } else {
            Ok(None)
        }
    }
}

impl Default for GCommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}
