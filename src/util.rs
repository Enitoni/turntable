use crate::{CommandList, Context, Error};
use chrono::prelude::*;

/// Calculates the latency of calling a command
#[poise::command(slash_command)]
async fn ping(ctx: Context<'_>) -> Result<(), Error> {
    let now = Utc::now();

    let incoming_latency = ctx
        .created_at()
        .signed_duration_since(now)
        .num_milliseconds() as f32;

    ctx.say(format!("Pong! ({}s)", incoming_latency.abs() / 1000.))
        .await?;

    Ok(())
}

pub fn commands() -> CommandList {
    vec![ping()]
}
