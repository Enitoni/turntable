use crate::discord::{Context, Error};
use poise::serenity_prelude::Mentionable;
use songbird::input::Input;

use super::CommandList;

/// Join the voice channel to stream audio
#[poise::command(slash_command)]
async fn join(ctx: Context<'_>) -> Result<(), Error> {
    let bot = ctx.data();

    ctx.defer().await?;

    let manager = songbird::get(ctx.discord())
        .await
        .expect("Failed to get Songbird");

    let channel = bot.voice_channel();

    let (handler, result) = manager.join(bot.home_guild(), channel).await;

    if result.is_ok() {
        let mut call = handler.lock().await;

        let input: Input = include_bytes!("../../assets/musikk.mp3").into();
        let handle = call.play_only_input(input);

        handle.set_volume(1.0)?;

        ctx.say(format!("Joined {}!", channel.mention())).await?;
    } else {
        ctx.say("Failed to join voice channel!").await?;
    }

    Ok(())
}

pub fn commands() -> CommandList {
    vec![join()]
}
