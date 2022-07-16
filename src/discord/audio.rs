use super::CommandList;
use crate::{
    discord::{Context, Error},
    ytdl::get_audio_url,
};

use youtube_dl::{YoutubeDl, YoutubeDlOutput};

/// Add a track to the queue
#[poise::command(slash_command)]
async fn play(
    ctx: Context<'_>,
    #[description = "Audio source"] source: String,
) -> Result<(), Error> {
    ctx.defer().await?;

    let url = get_audio_url(&source).await;

    if let Some(url) = url {
        ctx.say(url).await?;
    } else {
        ctx.say("No suitable source was found.").await?;
    }

    Ok(())
}

pub fn commands() -> CommandList {
    vec![play()]
}
