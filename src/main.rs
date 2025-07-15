use std::sync::Arc;

use slack_morphism::{
    SlackClient,
    prelude::{SlackClientHyperConnector, SlackHyperClient},
};

mod bridge;
mod commands;
mod redis;
mod sources;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let (channels, discord_rx, slack_rx) = bridge::create_bridge();
    let redis_client = redis::RedisClient::new()
        .await
        .expect("Failed to connect to Redis");

    let slack_client: Arc<SlackHyperClient> =
        Arc::new(SlackClient::new(SlackClientHyperConnector::new().unwrap()));

    tokio::join!(
        sources::discord::start(
            channels.clone(),
            discord_rx,
            redis_client.clone(),
            slack_client.clone()
        ),
        sources::slack::start(channels, slack_rx, redis_client, slack_client)
    );
}
