use crate::{CommandList, Context, Error};

/// Pong!
#[poise::command(slash_command)]
async fn ping(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say("Pong!").await?;

    Ok(())
}

pub fn commands() -> CommandList {
    vec![ping()]
}
