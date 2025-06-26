use dotenv_codegen::dotenv;
use poise::serenity_prelude as serenity;

struct Data {}
type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

const DISCORD_TOKEN: &str = dotenv!("DISCORD_TOKEN");
const GUILD_ID: &str = dotenv!("GUILD_ID");

#[poise::command(
    slash_command,
    prefix_command,
    description_localized("en-US", "Test the bot's latency.")
)]
async fn ping(ctx: Context<'_>) -> Result<(), Error> {
    ctx.send(
        poise::CreateReply::default()
            .content("🏓 pong!")
            .ephemeral(true)
            .reply(true),
    )
    .await?;

    Ok(())
}

#[tokio::main]
async fn main() {
    let intents = serenity::GatewayIntents::all();

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![ping()],
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some("c?".to_string()),
                mention_as_prefix: true,
                ..Default::default()
            },
            ..Default::default()
        })
        .setup(|ctx, ready, framework| {
            Box::pin(async move {
                println!("Logged in as {}", ready.user.name);

                poise::builtins::register_in_guild(
                    ctx,
                    &framework.options().commands,
                    serenity::GuildId::new(GUILD_ID.parse::<u64>()?),
                )
                .await?;

                Ok(Data {})
            })
        })
        .build();

    let client = serenity::ClientBuilder::new(DISCORD_TOKEN, intents)
        .framework(framework)
        .await;
    client.unwrap().start().await.unwrap();
}
