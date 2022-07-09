use std::{env, sync::Arc};

use crate::audio::AudioSystem;
use poise::{
    serenity_prelude::{ChannelId, Context as SerenityContext, GatewayIntents, GuildId},
    Event,
};
use songbird::{SerenityInit, Songbird};

use super::{util, voice, Context, Error, FrameworkContext};

pub struct Bot {
    pub audio: Arc<AudioSystem>,
    pub voice: Arc<Songbird>,
}

impl Bot {
    // This is GCT's Discords server
    const HOME_GUILD_ID: u64 = 671811819597201421;

    // The channel to join for streaming
    const VOICE_CHANNEL_ID: u64 = 671859933876191265;

    pub async fn run(audio: Arc<AudioSystem>) {
        let token = env::var("GCT_DISCORD_TOKEN").expect("GCT_DISCORD_TOKEN was not specified.");

        let intents = GatewayIntents::GUILDS
            | GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::GUILD_VOICE_STATES;

        let commands = {
            let mut list = vec![register()];

            list.extend(util::commands().into_iter());
            list.extend(voice::commands().into_iter());
            list
        };

        let bot = Bot {
            voice: Songbird::serenity(),
            audio,
        };

        let voice = bot.voice.clone();

        let framework = poise::Framework::build()
            .options(poise::FrameworkOptions {
                commands,
                listener: |ctx, event, framework, user_data| {
                    Box::pin(Bot::handle_event(ctx, event, framework, user_data))
                },
                ..Default::default()
            })
            .token(token)
            .intents(intents)
            .client_settings(move |client| client.register_songbird_with(voice))
            .user_data_setup(move |_ctx, _ready, _framework| Box::pin(async move { Ok(bot) }));

        framework.run().await.unwrap();
    }

    pub async fn handle_event(
        _ctx: &SerenityContext,
        event: &poise::Event<'_>,
        _framework: FrameworkContext<'_>,
        bot: &Bot,
    ) -> Result<(), Error> {
        if let Event::VoiceStateUpdate { old: _, new } = event {
            // Ensures that Songbird releases resources so buffers are not locked
            // DO NOT REMOVE THIS.
            if new.channel_id.is_none() {
                bot.voice.remove(bot.home_guild()).await?;
            }
        }

        Ok(())
    }

    pub fn home_guild(&self) -> GuildId {
        GuildId::new(Bot::HOME_GUILD_ID)
    }

    pub fn voice_channel(&self) -> ChannelId {
        ChannelId::new(Bot::VOICE_CHANNEL_ID)
    }
}

/// Update or delete application commands
#[poise::command(slash_command, owners_only)]
async fn register(ctx: Context<'_>) -> Result<(), Error> {
    poise::builtins::register_application_commands_buttons(ctx).await?;
    Ok(())
}
