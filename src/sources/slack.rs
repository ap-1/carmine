use std::{convert::Infallible, sync::Arc};

use axum::Extension;
use axum::body::Bytes;
use axum::http::Response;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Empty, Full};
use slack_morphism::prelude::*;
use slack_morphism::{
    SlackSigningSecret,
    prelude::{
        SlackClientEventsListenerEnvironment, SlackEventsAxumListener, SlackHyperClient,
        SlackHyperHttpsConnector, SlackHyperListenerEnvironment, SlackOAuthListenerConfig,
    },
};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use crate::bridge::{BridgeChannels, BridgeEvent, EventType};
use crate::commands::link::handle_link_channel;
use crate::commands::unlink::handle_unlink_channel;
use crate::redis::RedisClient;

async fn oauth_install_function(
    resp: SlackOAuthV2AccessTokenResponse,
    _client: Arc<SlackHyperClient>,
    _states: SlackClientEventsUserState,
) {
    println!("{resp:#?}");
}

async fn welcome_installed() -> String {
    "Welcome".to_string()
}

async fn cancelled_install() -> String {
    "Cancelled".to_string()
}

async fn error_install() -> String {
    "Error while installing".to_string()
}

async fn push_event(
    Extension(_environment): Extension<Arc<SlackHyperListenerEnvironment>>,
    Extension(bridge): Extension<Arc<BridgeChannels>>,
    Extension(slack_client): Extension<Arc<SlackHyperClient>>,
    Extension(event): Extension<SlackPushEvent>,
) -> Response<BoxBody<Bytes, Infallible>> {
    println!("Received push event: {event:?}");

    match event {
        SlackPushEvent::UrlVerification(url_ver) => {
            println!("URL verification challenge received");
            Response::new(Full::new(url_ver.challenge.into()).boxed())
        }
        SlackPushEvent::EventCallback(SlackPushEventCallback {
            event: SlackEventCallbackBody::Message(message_event),
            team_id,
            ..
        }) => {
            if let Some(bridge_event) =
                create_bridge_event(message_event, team_id, slack_client).await
                && let Err(e) = bridge.to_discord.send(bridge_event)
            {
                eprintln!("Failed to send bridge event: {e}");
            }

            Response::new(Empty::new().boxed())
        }
        _ => {
            println!("Other event type: {event:?}");
            Response::new(Empty::new().boxed())
        }
    }
}

async fn get_user_info(
    user_id: Option<SlackUserId>,
    slack_client: Arc<SlackHyperClient>,
) -> (String, String) {
    let oauth_token = std::env::var("SLACK_OAUTH_TOKEN").expect("SLACK_OAUTH_TOKEN must be set");
    let slack_token = SlackApiToken::new(oauth_token.into());

    let session = slack_client.open_session(&slack_token);

    if let Some(user_id) = user_id
        && let Ok(response) = session
            .users_info(&SlackApiUsersInfoRequest::new(user_id))
            .await
        && let Some(profile) = response.user.profile
    {
        let name = profile
            .display_name
            .or(profile.real_name)
            .unwrap_or_else(|| "Unknown User".to_string());

        let avatar = profile
            .icon
            .and_then(|i| i.image_original)
            .unwrap_or_default();

        (name, avatar)
    } else {
        ("Unknown User".to_string(), "".to_string())
    }
}

async fn create_bridge_event(
    message_event: SlackMessageEvent,
    team_id: SlackTeamId,
    slack_client: Arc<SlackHyperClient>,
) -> Option<BridgeEvent> {
    // Extract common metadata
    let message_ts = message_event.origin.ts.to_string();
    let channel_id = message_event.origin.channel?.to_string();
    let team_id_str = team_id.to_string();

    // Extract user info
    let user_id = message_event.sender.user;
    let (author_name, author_avatar) = get_user_info(user_id, slack_client).await;

    let event_type = match message_event.subtype {
        None => {
            // Regular message
            let content = message_event
                .content?
                .text
                .unwrap_or_else(|| "Failed to get message content".to_string());

            EventType::MessageSent {
                message_id: message_ts.clone(),
                content,
            }
        }
        Some(SlackMessageEventType::MessageChanged) => {
            // For edited messages, the new content is in message.content, not the content field
            let new_content = message_event
                .message?
                .content?
                .text
                .unwrap_or_else(|| "Failed to get message content".to_string());

            EventType::MessageEdited {
                message_id: message_ts.clone(),
                new_content,
            }
        }
        Some(SlackMessageEventType::MessageDeleted) => {
            let deleted_message_id = message_event
                .deleted_ts
                .map(|ts| ts.to_string())
                .expect("Deleted message did not have a timestamp");

            EventType::MessageDeleted {
                message_id: deleted_message_id,
            }
        }
        _ => {
            println!("Unhandled message subtype: {:?}", message_event.subtype);
            return None;
        }
    };

    Some(BridgeEvent {
        event_type,
        author_name,
        author_avatar,
        channel_id,
        team_id: team_id_str,
    })
}

async fn command_event(
    Extension(_environment): Extension<Arc<SlackHyperListenerEnvironment>>,
    Extension(redis_client): Extension<Arc<RedisClient>>,
    Extension(event): Extension<SlackCommandEvent>,
) -> axum::Json<SlackCommandEventResponse> {
    println!("Received command event: {event:?}");

    let response = match event.command.0.as_str() {
        "/link-channel" => handle_link_channel(event, redis_client).await,
        "/unlink-channel" => handle_unlink_channel(event, redis_client).await,
        "/help" => SlackCommandEventResponse::new(
            SlackMessageContent::new()
                .with_text("Available commands: /link-channel, /unlink-channel, /help".into()),
        ),
        _ => SlackCommandEventResponse::new(
            SlackMessageContent::new().with_text("Unknown command".into()),
        ),
    };

    axum::Json(response)
}

async fn interaction_event(
    Extension(_environment): Extension<Arc<SlackHyperListenerEnvironment>>,
    Extension(event): Extension<SlackInteractionEvent>,
) {
    println!("Received interaction event: {event:?}");
}

fn error_handler(
    err: Box<dyn std::error::Error + Send + Sync>,
    _client: Arc<SlackHyperClient>,
    _states: SlackClientEventsUserState,
) -> HttpStatusCode {
    println!("{err:#?}");

    // Defines what we return Slack server
    HttpStatusCode::BAD_REQUEST
}

pub async fn start(
    channels: BridgeChannels,
    slack_rx: mpsc::UnboundedReceiver<BridgeEvent>,
    redis_client: RedisClient,
    slack_client: Arc<SlackHyperClient>,
) {
    let slack_client_id = std::env::var("SLACK_CLIENT_ID").expect("SLACK_CLIENT_ID must be set");
    let slack_client_secret =
        std::env::var("SLACK_CLIENT_SECRET").expect("SLACK_CLIENT_SECRET must be set");
    let slack_bot_scope = std::env::var("SLACK_BOT_SCOPE").expect("SLACK_BOT_SCOPE must be set");
    let slack_redirect_host =
        std::env::var("SLACK_REDIRECT_HOST").expect("SLACK_REDIRECT_HOST must be set");
    let slack_signing_secret =
        std::env::var("SLACK_SIGNING_SECRET").expect("SLACK_SIGNING_SECRET must be set");

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], 8080));
    println!("Loading server: {addr}");

    let oauth_listener_config = SlackOAuthListenerConfig::new(
        slack_client_id.into(),
        slack_client_secret.into(),
        slack_bot_scope,
        slack_redirect_host,
    );

    let listener_environment: Arc<SlackHyperListenerEnvironment> = Arc::new(
        SlackClientEventsListenerEnvironment::new(slack_client.clone())
            .with_error_handler(error_handler),
    );
    let signing_secret: SlackSigningSecret = slack_signing_secret.into();

    let listener: SlackEventsAxumListener<SlackHyperHttpsConnector> =
        SlackEventsAxumListener::new(listener_environment.clone());
    let bridge_channels = Arc::new(channels);

    // Build application route with OAuth nested router and Push/Command/Interaction events
    let app = axum::routing::Router::new()
        .nest(
            "/auth",
            listener.oauth_router("/auth", &oauth_listener_config, oauth_install_function),
        )
        .route("/installed", axum::routing::get(welcome_installed))
        .route("/cancelled", axum::routing::get(cancelled_install))
        .route("/error", axum::routing::get(error_install))
        .route(
            "/push",
            axum::routing::post(push_event).layer(
                listener
                    .events_layer(&signing_secret)
                    .with_event_extractor(SlackEventsExtractors::push_event()),
            ),
        )
        .route(
            "/command",
            axum::routing::post(command_event).layer(
                listener
                    .events_layer(&signing_secret)
                    .with_event_extractor(SlackEventsExtractors::command_event()),
            ),
        )
        .route(
            "/interaction",
            axum::routing::post(interaction_event).layer(
                listener
                    .events_layer(&signing_secret)
                    .with_event_extractor(SlackEventsExtractors::interaction_event()),
            ),
        )
        .layer(Extension(bridge_channels))
        .layer(Extension(redis_client))
        .layer(Extension(slack_client));

    axum::serve(TcpListener::bind(&addr).await.unwrap(), app)
        .await
        .unwrap();
}
