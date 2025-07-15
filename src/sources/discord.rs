use std::sync::Arc;

use poise::serenity_prelude::EditWebhookMessage;
use poise::serenity_prelude::{
    self as serenity, CreateWebhook, EventHandler, ExecuteWebhook, prelude::TypeMapKey,
};
use slack_morphism::prelude::SlackHyperClient;
use tokio::sync::mpsc;

use crate::bridge::{BridgeChannels, BridgeEvent, EventType};
use crate::commands::{general::help, link::link_channel, unlink::unlink_channel};
use crate::redis::RedisClient;

#[derive(Clone)]
pub struct Data {
    pub bridge: BridgeChannels,
    pub redis_client: RedisClient,
    pub slack_client: Arc<SlackHyperClient>,
}

impl TypeMapKey for Data {
    type Value = Data;
}

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;

struct Handler;

#[serenity::async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: serenity::Context, msg: serenity::Message) {
        println!("Received message: {}", msg.content);
    }

    async fn message_delete(
        &self,
        ctx: serenity::Context,
        channel_id: serenity::ChannelId,
        message_id: serenity::MessageId,
        guild_id: Option<serenity::GuildId>,
    ) {
        println!("Message deleted: {message_id} in channel {channel_id}");
    }

    async fn message_update(
        &self,
        ctx: serenity::Context,
        old: Option<serenity::Message>,
        new: Option<serenity::Message>,
        _event: serenity::MessageUpdateEvent,
    ) {
        println!("Message updated: {new:?}");
    }
}

async fn get_discord_channel_id(slack_channel_id: &str, redis_client: &RedisClient) -> Option<u64> {
    match redis_client
        .get_linked_discord_channel(slack_channel_id)
        .await
    {
        Ok(Some(channel_str)) => match channel_str.parse::<u64>() {
            Ok(id) => Some(id),
            Err(_) => {
                eprintln!("Invalid Discord channel ID format: {channel_str}");
                None
            }
        },
        Ok(None) => {
            eprintln!("No linked Discord channel found for Slack channel: {slack_channel_id}");
            None
        }
        Err(e) => {
            eprintln!("Error fetching Discord channel ID: {e}");
            None
        }
    }
}

async fn get_or_create_webhook(
    ctx: &serenity::Context,
    channel_id: u64,
) -> Option<serenity::Webhook> {
    let channel = serenity::ChannelId::new(channel_id);

    let webhooks = match channel.webhooks(&ctx.http).await {
        Ok(webhooks) => webhooks,
        Err(e) => {
            eprintln!("Failed to get webhooks: {e}");
            return None;
        }
    };

    // Check for existing webhook
    if let Some(existing) = webhooks
        .iter()
        .find(|w| w.name.as_deref() == Some("carmine"))
    {
        return Some(existing.clone());
    }

    // Create new webhook
    match channel
        .create_webhook(&ctx.http, CreateWebhook::new("carmine"))
        .await
    {
        Ok(webhook) => Some(webhook),
        Err(e) => {
            eprintln!("Failed to create webhook: {e}");
            None
        }
    }
}

async fn send_message_to_discord(
    ctx: &serenity::Context,
    event: &BridgeEvent,
    content: &str,
    redis_client: &RedisClient,
) -> Option<serenity::Message> {
    // Find linked Discord channel
    let channel_id = match get_discord_channel_id(&event.channel_id, redis_client).await {
        Some(id) => id,
        None => return None,
    };

    // Get or create webhook
    let webhook = match get_or_create_webhook(ctx, channel_id).await {
        Some(webhook) => webhook,
        None => return None,
    };

    // Send message via webhook
    match webhook
        .execute(
            &ctx.http,
            true,
            ExecuteWebhook::new()
                .username(&event.author_name)
                .avatar_url(&event.author_avatar)
                .content(content),
        )
        .await
    {
        Ok(Some(message)) => Some(message),
        Ok(None) => {
            eprintln!("Webhook execution returned no message");
            None
        }
        Err(e) => {
            eprintln!("Failed to send webhook message: {e}");
            None
        }
    }
}

async fn handle_message_deletion(
    ctx: &serenity::Context,
    slack_message_ts: &str,
    redis_client: &RedisClient,
) {
    // Look up Discord message from Slack timestamp
    if let Ok(Some(discord_info)) = redis_client.get_discord_message(slack_message_ts).await {
        let [channel_id, message_id] = match discord_info
            .split(':')
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(vec) if vec.len() == 2 => [vec[0], vec[1]],
            _ => {
                eprintln!("Invalid Discord message mapping: {discord_info}");
                return;
            }
        };

        let channel = serenity::ChannelId::new(channel_id);
        if let Err(e) = channel.delete_message(&ctx.http, message_id).await {
            eprintln!("Failed to delete Discord message: {e}");
        }

        // Clean up mapping
        let _ = redis_client
            .delete_message_mapping_from_slack(slack_message_ts)
            .await;
    }
}

async fn handle_message_edit(
    ctx: &serenity::Context,
    slack_message_ts: &str,
    new_content: &str,
    redis_client: &RedisClient,
) {
    // Look up Discord message from Slack timestamp
    if let Ok(Some(discord_info)) = redis_client.get_discord_message(slack_message_ts).await {
        let [channel_id, message_id] = match discord_info
            .split(':')
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(vec) if vec.len() == 2 => [vec[0], vec[1]],
            _ => {
                eprintln!("Invalid Discord message mapping: {discord_info}");
                return;
            }
        };

        let webhook = match get_or_create_webhook(ctx, channel_id).await {
            Some(webhook) => webhook,
            None => {
                eprintln!("Failed to get or create webhook for editing message");
                return;
            }
        };

        if let Err(e) = webhook
            .edit_message(
                &ctx.http,
                message_id.into(),
                EditWebhookMessage::new().content(new_content),
            )
            .await
        {
            eprintln!("Failed to edit Discord message via webhook: {}", e);
        }
    }
}

async fn handle_bridge_event(
    ctx: &serenity::Context,
    team_id: &str,
    event: BridgeEvent,
    redis_client: &RedisClient,
) {
    if event.team_id != team_id {
        return; // Ignore events from other workspaces
    }

    match &event.event_type {
        EventType::MessageSent {
            content,
            message_id,
        } => {
            if let Some(discord_message) =
                send_message_to_discord(ctx, &event, content, &redis_client).await
            {
                // Store message mapping in Redis
                if let Err(e) = redis_client
                    .store_message_mapping(
                        discord_message.channel_id.into(),
                        discord_message.id.into(),
                        &event.channel_id,
                        message_id,
                    )
                    .await
                {
                    eprintln!("Failed to store message mapping: {e}");
                }
            }
        }
        EventType::MessageDeleted { message_id } => {
            handle_message_deletion(ctx, message_id, &redis_client).await;
        }
        EventType::MessageEdited {
            message_id,
            new_content,
        } => {
            handle_message_edit(ctx, message_id, new_content, &redis_client).await;
        }
        _ => {
            println!("Unhandled event type: {:?}", event.event_type);
        }
    }
}

pub async fn start(
    channels: BridgeChannels,
    mut discord_rx: mpsc::UnboundedReceiver<BridgeEvent>,
    redis_client: RedisClient,
    slack_client: Arc<SlackHyperClient>,
) {
    let discord_token = std::env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN must be set");
    let guild_id: u64 = std::env::var("DISCORD_GUILD_ID")
        .expect("DISCORD_GUILD_ID must be set")
        .parse()
        .expect("DISCORD_GUILD_ID must be a valid u64");
    let team_id = std::env::var("SLACK_TEAM_ID").expect("SLACK_TEAM_ID must be set");

    let intents = serenity::GatewayIntents::all();
    let data = Data {
        bridge: channels,
        redis_client: redis_client.clone(),
        slack_client,
    };

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![help(), link_channel(), unlink_channel()],
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

                let ctx_for_handler = ctx.clone();
                tokio::spawn(async move {
                    while let Some(event) = discord_rx.recv().await {
                        handle_bridge_event(&ctx_for_handler, &team_id, event, &redis_client).await;
                    }
                });

                Ok(data)
            })
        })
        .build();

    let client = serenity::ClientBuilder::new(discord_token, intents)
        .event_handler(Handler)
        .framework(framework)
        .await;
    client.unwrap().start().await.unwrap();
}
