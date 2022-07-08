use super::{CommandList, Context, Error};
use chrono::prelude::*;

/// Calculates the latency of calling a command
#[poise::command(slash_command)]
async fn ping(ctx: Context<'_>) -> Result<(), Error> {
    let now = Utc::now();

    let incoming_latency = ctx.created_at().unix_timestamp_nanos() / 1_000_000;
    let milliseconds = (incoming_latency as i64 - now.timestamp_millis()) as f32;

    ctx.say(format!("Pong! ({}s)", milliseconds.abs() / 1000.))
        .await?;

    Ok(())
}

pub fn commands() -> CommandList {
    vec![ping()]
}
