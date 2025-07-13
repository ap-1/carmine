use poise::{serenity_prelude as serenity};

use crate::commands::general::{help, ping};

pub struct Data {}
pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;

pub async fn start() {
    let discord_token = std::env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN must be set");
    let guild_id: u64 = std::env::var("DISCORD_GUILD_ID")
        .expect("DISCORD_GUILD_ID must be set")
        .parse()
        .expect("DISCORD_GUILD_ID must be a valid u64");

    let intents = serenity::GatewayIntents::all();

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![ping(), help()],
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some("c?".to_string()),
                mention_as_prefix: true,
                ..Default::default()
            },
            ..Default::default()
        })
        .setup(move |ctx, ready, framework| {
            Box::pin(async move {
                println!("Logged in as {}", ready.user.name);

                poise::builtins::register_in_guild(
                    ctx,
                    &framework.options().commands,
                    serenity::GuildId::new(guild_id),
                )
                .await?;

                Ok(Data {})
            })
        })
        .build();

    let client = serenity::ClientBuilder::new(discord_token, intents)
        .framework(framework)
        .await;
    client.unwrap().start().await.unwrap();
}
