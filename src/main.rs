use serenity::model::gateway::Ready;
use serenity::prelude::*;
use std::env;

const INTENTS: GatewayIntents = GatewayIntents::GUILDS;

struct Bot;

#[serenity::async_trait]
impl serenity::client::EventHandler for Bot {
    async fn ready(&self, _ctx: Context, _data_about_bot: Ready) {
        println!("GCT is ready.")
    }
}

#[tokio::main]
async fn main() {
    let token = env::var("GCT_DISCORD_TOKEN").expect("GCT_DISCORD_TOKEN was not specified.");

    let mut client = Client::builder(&token, INTENTS)
        .event_handler(Bot)
        .await
        .expect("Could not create client");

    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}
