use crate::discord::{Context, Error};
use poise::serenity_prelude::Mentionable;

use super::CommandList;

/// Join the voice channel to stream audio
#[poise::command(slash_command)]
async fn join(ctx: Context<'_>) -> Result<(), Error> {
    let bot = ctx.data();

    ctx.defer().await?;

    let channel = bot.voice_channel();
    let (handler, result) = bot.voice.join(bot.home_guild(), channel).await;

    if result.is_ok() {
        let mut call = handler.lock().await;

        let input = bot.audio.create_input();
        let handle = call.play_only_input(input);

        handle.set_volume(1.0)?;
        handle.enable_loop();

        ctx.say(format!("Joined {}!", channel.mention())).await?;
    } else {
        ctx.say("Failed to join voice channel!").await?;
    }

    Ok(())
}

pub fn commands() -> CommandList {
    vec![join()]
}
