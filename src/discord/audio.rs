use super::CommandList;
use crate::{
    audio,
    discord::{Context, Error},
    ytdl::get_audio_url,
};

/// Add a track to the queue
#[poise::command(slash_command)]
async fn play(
    ctx: Context<'_>,
    #[description = "Audio source"] source: String,
) -> Result<(), Error> {
    ctx.defer().await?;

    let input = audio::Input::parse(&source);

    if let Some(input) = input {
        ctx.say(&input).await?;
    } else {
        ctx.say("No suitable source was found.").await?;
    }

    Ok(())
}

pub fn commands() -> CommandList {
    vec![play()]
}
