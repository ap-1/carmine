use poise::serenity_prelude::{CreateAllowedMentions};

use crate::discord::{Context, Error};

#[poise::command(
    prefix_command,
    slash_command,
    description_localized("en-US", "Ping the bot.")
)]
pub async fn ping(ctx: Context<'_>) -> Result<(), Error> {
    ctx.send(
        poise::CreateReply::default()
            .content("🏓 pong!")
            .ephemeral(true)
            .allowed_mentions(CreateAllowedMentions::new().replied_user(false))
            .reply(true),
    )
    .await?;

    Ok(())
}

#[poise::command(prefix_command, track_edits, slash_command)]
pub async fn help(
    ctx: Context<'_>,
    #[description = "Specific command to show help about"]
    #[autocomplete = "poise::builtins::autocomplete_command"]
    command: Option<String>,
) -> Result<(), Error> {
    poise::builtins::help(
        ctx,
        command.as_deref(),
        poise::builtins::HelpConfiguration::default(),
    )
    .await?;
    Ok(())
}
