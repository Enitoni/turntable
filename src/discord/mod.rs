//! The discord part of GCT is implemented in this module.

mod bot;
mod compat;
mod util;
mod voice;

pub use bot::*;
pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Bot, Error>;
// pub type FrameworkContext<'a> = poise::FrameworkContext<'a, Bot, Error>;
pub type CommandList = Vec<poise::Command<Bot, Error>>;
