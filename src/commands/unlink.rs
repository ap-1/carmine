use std::sync::Arc;

use poise::serenity_prelude::CreateAllowedMentions;
use slack_morphism::{
    SlackMessageContent,
    events::{SlackCommandEvent, SlackCommandEventResponse},
};

use crate::{
    redis::RedisClient,
    sources::discord::{Context, Error},
};

// Slack command
pub async fn handle_unlink_channel(
    event: SlackCommandEvent,
    redis_client: Arc<RedisClient>,
) -> SlackCommandEventResponse {
    let channel_id = event.channel_id.to_string();

    // See if there's a linked Discord channel
    if let Ok(Some(discord_channel_str)) =
        redis_client.get_linked_discord_channel(&channel_id).await
        && let Ok(discord_channel_id) = discord_channel_str.trim().parse::<u64>()
    {
        // Remove the link from Redis
        match redis_client
            .unlink_channels(discord_channel_id, &channel_id)
            .await
        {
            Ok(_) => SlackCommandEventResponse::new(SlackMessageContent::new().with_text(format!(
                "Successfully unlinked Discord channel `{discord_channel_id}` from this Slack channel"
            ))),
            Err(e) => SlackCommandEventResponse::new(
                SlackMessageContent::new()
                    .with_text(format!("Error unlinking Discord channel: {e}")),
            ),
        }
    } else {
        SlackCommandEventResponse::new(
            SlackMessageContent::new()
                .with_text("This Slack channel is not linked to any Discord channel".into()),
        )
    }
}

// Discord command
#[poise::command(
    prefix_command,
    slash_command,
    description_localized("en-US", "Unlink a Slack channel from this Discord channel.")
)]
pub async fn unlink_channel(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    let redis_client = &data.redis_client;

    let channel_id: u64 = ctx.channel_id().into();

    // See if there's a linked Slack channel
    match redis_client.get_linked_slack_channel(channel_id).await? {
        Some(slack_channel_id) => {
            // Remove the link from Redis
            redis_client
                .unlink_channels(channel_id, &slack_channel_id)
                .await?;

            ctx.send(
                poise::CreateReply::default()
                    .content(format!(
                        "Successfully unlinked Slack channel **`{slack_channel_id}`** from this Discord channel"
                    ))
                    .allowed_mentions(CreateAllowedMentions::new().replied_user(false))
                    .reply(true),
            )
            .await?;
        }
        None => {
            ctx.send(
                poise::CreateReply::default()
                    .content("This Discord channel is not linked to any Slack channel")
                    .allowed_mentions(CreateAllowedMentions::new().replied_user(false))
                    .reply(true),
            )
            .await?;
        }
    }

    Ok(())
}
