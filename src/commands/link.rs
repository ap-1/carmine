use std::sync::Arc;

use poise::serenity_prelude::CreateAllowedMentions;
use slack_morphism::api::{SlackApiConversationsInfoRequest, SlackApiConversationsJoinRequest};
use slack_morphism::events::{SlackCommandEvent, SlackCommandEventResponse};
use slack_morphism::prelude::SlackHyperClient;
use slack_morphism::{SlackApiToken, SlackChannelId, SlackMessageContent};

use crate::redis::RedisClient;
use crate::sources::discord::{Context, Error};

// Slack command
pub async fn handle_link_channel(
    event: SlackCommandEvent,
    redis_client: Arc<RedisClient>,
) -> SlackCommandEventResponse {
    if let Some(channel_id) = event.text
        && !channel_id.is_empty()
        && let Ok(discord_channel_id) = channel_id.trim().parse::<u64>()
    {
        // Store the mapping in Redis
        match redis_client
            .link_channels(discord_channel_id, event.channel_id.as_ref())
            .await
        {
            Ok(_) => SlackCommandEventResponse::new(SlackMessageContent::new().with_text(format!(
                "Successfully linked Discord channel `{discord_channel_id}` to this Slack channel"
            ))),
            Err(e) => SlackCommandEventResponse::new(
                SlackMessageContent::new().with_text(format!("Error linking Discord channel: {e}")),
            ),
        }
    } else {
        SlackCommandEventResponse::new(
            SlackMessageContent::new()
                .with_text("Please provide a valid Discord channel ID".into()),
        )
    }
}

// Discord command
#[poise::command(
    prefix_command,
    slash_command,
    description_localized("en-US", "Link a Slack channel to this Discord channel.")
)]
pub async fn link_channel(
    ctx: Context<'_>,
    #[description = "Slack channel ID (e.g., C1234567890)"] slack_channel_id: String,
) -> Result<(), Error> {
    let data = ctx.data();
    let redis_client = &data.redis_client;

    let bot_token = std::env::var("SLACK_BOT_TOKEN").expect("SLACK_BOT_TOKEN must be set");
    let slack_token = SlackApiToken::new(bot_token.into());

    match verify_and_join_slack_channel(&data.slack_client, &slack_token, &slack_channel_id).await {
        Ok(channel_name) => {
            // Store the mapping in Redis
            redis_client
                .link_channels(ctx.channel_id().into(), &slack_channel_id)
                .await?;

            ctx.send(
                poise::CreateReply::default()
                    .content(format!(
                        "Successfully linked Slack channel **`{channel_name}`** to this Discord channel",
                    ))
                    .allowed_mentions(CreateAllowedMentions::new().replied_user(false))
                    .reply(true),
            )
            .await?;

            Ok(())
        }
        Err(e) => {
            ctx.send(
                poise::CreateReply::default()
                    .content(format!("Error linking Slack channel: {e}"))
                    .allowed_mentions(CreateAllowedMentions::new().replied_user(false))
                    .reply(true),
            )
            .await?;

            Err(Error::from(e))
        }
    }
}

async fn verify_and_join_slack_channel(
    client: &SlackHyperClient,
    token: &SlackApiToken,
    channel_id: &str,
) -> Result<String, String> {
    let session = client.open_session(token);
    let channel_info_req =
        SlackApiConversationsInfoRequest::new(SlackChannelId::new(channel_id.to_string()));

    // Verify the channel exists and get its name
    let channel_info = session
        .conversations_info(&channel_info_req)
        .await
        .map_err(|e| format!("Failed to get channel info: {e}"))?;
    let channel_name = channel_info
        .channel
        .name
        .unwrap_or("Unknown".to_string())
        .to_string();

    // Join the channel if not already a member
    match channel_info.channel.flags.is_member {
        Some(true) => Ok(channel_name),
        _ => {
            let join_req =
                SlackApiConversationsJoinRequest::new(SlackChannelId::new(channel_id.to_string()));

            match session.conversations_join(&join_req).await {
                Ok(_) => Ok(channel_name),
                Err(e) => Err(format!("Failed to join channel '{channel_name}': {e}")),
            }
        }
    }
}
