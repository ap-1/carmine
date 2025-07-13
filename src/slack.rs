use axum::Extension;
use axum::body::Bytes;
use axum::http::Response;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Empty, Full};
use slack_morphism::prelude::*;
use slack_morphism::{
    SlackClient, SlackSigningSecret,
    prelude::{
        SlackClientEventsListenerEnvironment, SlackClientHyperConnector, SlackEventsAxumListener,
        SlackHyperClient, SlackHyperHttpsConnector, SlackHyperListenerEnvironment,
        SlackOAuthListenerConfig,
    },
};
use std::{convert::Infallible, sync::Arc};
use tokio::net::TcpListener;

async fn oauth_install_function(
    resp: SlackOAuthV2AccessTokenResponse,
    _client: Arc<SlackHyperClient>,
    _states: SlackClientEventsUserState,
) {
    println!("{:#?}", resp);
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
    Extension(event): Extension<SlackPushEvent>,
) -> Response<BoxBody<Bytes, Infallible>> {
    println!("Received push event: {:?}", event);

    match event {
        SlackPushEvent::UrlVerification(url_ver) => {
            println!("URL verification challenge received");
            Response::new(Full::new(url_ver.challenge.into()).boxed())
        }
        SlackPushEvent::EventCallback(SlackPushEventCallback {
            event: SlackEventCallbackBody::Message(message_event),
            ..
        }) => {
            println!("New message received");
            println!("Channel: {:?}", message_event.origin.channel);
            println!("Origin: {:?}", message_event.origin);
            println!("Content: {:?}", message_event.content);

            Response::new(Empty::new().boxed())
        }
        _ => {
            println!("Other event type: {:?}", event);
            Response::new(Empty::new().boxed())
        }
    }
}

async fn command_event(
    Extension(_environment): Extension<Arc<SlackHyperListenerEnvironment>>,
    Extension(event): Extension<SlackCommandEvent>,
) -> axum::Json<SlackCommandEventResponse> {
    println!("Received command event: {:?}", event);
    axum::Json(SlackCommandEventResponse::new(
        SlackMessageContent::new().with_text("Working on it".into()),
    ))
}

async fn interaction_event(
    Extension(_environment): Extension<Arc<SlackHyperListenerEnvironment>>,
    Extension(event): Extension<SlackInteractionEvent>,
) {
    println!("Received interaction event: {:?}", event);
}

fn error_handler(
    err: Box<dyn std::error::Error + Send + Sync>,
    _client: Arc<SlackHyperClient>,
    _states: SlackClientEventsUserState,
) -> HttpStatusCode {
    println!("{:#?}", err);

    // Defines what we return Slack server
    HttpStatusCode::BAD_REQUEST
}

pub async fn start() {
    let slack_client_id = std::env::var("SLACK_CLIENT_ID").expect("SLACK_CLIENT_ID must be set");
    let slack_client_secret =
        std::env::var("SLACK_CLIENT_SECRET").expect("SLACK_CLIENT_SECRET must be set");
    let slack_bot_scope = std::env::var("SLACK_BOT_SCOPE").expect("SLACK_BOT_SCOPE must be set");
    let slack_redirect_host =
        std::env::var("SLACK_REDIRECT_HOST").expect("SLACK_REDIRECT_HOST must be set");
    let slack_signing_secret =
        std::env::var("SLACK_SIGNING_SECRET").expect("SLACK_SIGNING_SECRET must be set");

    let client: Arc<SlackHyperClient> =
        Arc::new(SlackClient::new(SlackClientHyperConnector::new().unwrap()));

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], 8080));
    println!("Loading server: {}", addr);

    let oauth_listener_config = SlackOAuthListenerConfig::new(
        slack_client_id.into(),
        slack_client_secret.into(),
        slack_bot_scope.into(),
        slack_redirect_host.into(),
    );

    let listener_environment: Arc<SlackHyperListenerEnvironment> = Arc::new(
        SlackClientEventsListenerEnvironment::new(client.clone()).with_error_handler(error_handler),
    );
    let signing_secret: SlackSigningSecret = slack_signing_secret.into();

    let listener: SlackEventsAxumListener<SlackHyperHttpsConnector> =
        SlackEventsAxumListener::new(listener_environment.clone());

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
        );

    axum::serve(TcpListener::bind(&addr).await.unwrap(), app)
        .await
        .unwrap();
}
