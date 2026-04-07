use crate::gcommands::{GCommand, GContext};
use std::future::Future;
use std::pin::Pin;
use tracing::info;

pub struct Pop;

impl GCommand for Pop {
    fn name(&self) -> &'static str {
        "pop"
    }

    fn execute<'a>(
        &'a self,
        ctx: GContext<'a>,
        _args: &'a [&'a str],
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Option<String>>> + Send + 'a>> {
        Box::pin(async move {
            info!("Executing 'pop' command for {}:{}", ctx.ip, ctx.port);

            match ctx
                .data
                .battlemetrics
                .get_server_pop(&ctx.ip, ctx.port)
                .await
            {
                Ok(pop) => Ok(Some(format!("Server Population: {pop}"))),
                Err(e) => {
                    tracing::error!("Failed to get population: {e}");
                    Ok(Some(
                        "Failed to fetch population from Battlemetrics.".to_string(),
                    ))
                }
            }
        })
    }
}
