use redis::{AsyncTypedCommands, ErrorKind, RedisError};
use redis::{Client, RedisResult};

#[derive(Debug, Clone)]
pub struct RedisClient {
    client: Client,
}

impl RedisClient {
    pub async fn new() -> RedisResult<Self> {
        let redis_url = std::env::var("REDIS_URL").expect("REDIS_URL must be set");
        let client = Client::open(redis_url)?;

        Ok(Self { client })
    }

    async fn get_connection(&self) -> RedisResult<redis::aio::MultiplexedConnection> {
        self.client.get_multiplexed_async_connection().await
    }

    // Channel linking
    pub async fn link_channels(
        &self,
        discord_channel_id: u64,
        slack_channel_id: &str,
    ) -> RedisResult<()> {
        let mut conn = self.get_connection().await?;

        let discord_key = format!("discord_channel:{discord_channel_id}:slack");
        let slack_key = format!("slack_channel:{slack_channel_id}:discord");

        conn.set(&discord_key, slack_channel_id).await?;
        conn.set(&slack_key, discord_channel_id).await?;

        Ok(())
    }

    pub async fn unlink_channels(
        &self,
        discord_channel_id: u64,
        slack_channel_id: &str,
    ) -> RedisResult<()> {
        let mut conn = self.get_connection().await?;

        let discord_key = format!("discord_channel:{discord_channel_id}:slack");
        let slack_key = format!("slack_channel:{slack_channel_id}:discord");

        conn.del(&discord_key).await?;
        conn.del(&slack_key).await?;

        Ok(())
    }

    pub async fn get_linked_slack_channel(
        &self,
        discord_channel_id: u64,
    ) -> RedisResult<Option<String>> {
        let mut conn = self.get_connection().await?;
        let key = format!("discord_channel:{discord_channel_id}:slack");
        conn.get(&key).await
    }

    pub async fn get_linked_discord_channel(
        &self,
        slack_channel_id: &str,
    ) -> RedisResult<Option<String>> {
        let mut conn = self.get_connection().await?;
        let key = format!("slack_channel:{slack_channel_id}:discord");
        conn.get(&key).await
    }

    // Message mapping methods
    pub async fn store_message_mapping(
        &self,
        discord_channel_id: u64,
        discord_message_id: u64,
        slack_channel_id: &str,
        slack_message_ts: &str,
    ) -> RedisResult<()> {
        let mut conn = self.get_connection().await?;

        let discord_info = format!("{discord_channel_id}:{discord_message_id}");
        let slack_info = format!("{slack_channel_id}:{slack_message_ts}");

        conn.set(&format!("discord_msg:{discord_message_id}"), &slack_info)
            .await?;
        conn.set(&format!("slack_msg:{slack_message_ts}"), &discord_info)
            .await?;

        Ok(())
    }

    pub async fn get_slack_message(&self, discord_message_id: u64) -> RedisResult<Option<String>> {
        let mut conn = self.get_connection().await?;
        conn.get(&format!("discord_msg:{discord_message_id}")).await
    }

    pub async fn get_discord_message(&self, slack_message_ts: &str) -> RedisResult<Option<String>> {
        let mut conn = self.get_connection().await?;
        conn.get(&format!("slack_msg:{slack_message_ts}")).await
    }

    pub async fn delete_message_mapping_from_slack(
        &self,
        slack_message_ts: &str,
    ) -> RedisResult<()> {
        let mut conn = self.get_connection().await?;

        // Delete discord message mapping first
        if let Some(discord_info) = self.get_discord_message(slack_message_ts).await? {
            match discord_info.split(':').collect::<Vec<_>>().as_slice() {
                [_, message_id] => {
                    conn.del(&format!("discord_msg:{message_id}")).await?;
                }
                _ => {
                    return Err(RedisError::from((
                        ErrorKind::TypeError,
                        "Invalid Discord message mapping format",
                    )));
                }
            }
        }

        conn.del(&format!("slack_msg:{slack_message_ts}")).await?;
        Ok(())
    }

    pub async fn delete_message_mapping_from_discord(
        &self,
        discord_message_id: u64,
    ) -> RedisResult<()> {
        let mut conn = self.get_connection().await?;

        // Delete slack message mapping first
        if let Some(slack_info) = self.get_slack_message(discord_message_id).await? {
            match slack_info.split(':').collect::<Vec<_>>().as_slice() {
                [_, slack_message_ts] => {
                    conn.del(&format!("slack_msg:{slack_message_ts}")).await?;
                }
                _ => {
                    return Err(RedisError::from((
                        ErrorKind::TypeError,
                        "Invalid Slack message mapping format",
                    )));
                }
            }
        }

        conn.del(&format!("discord_msg:{discord_message_id}"))
            .await?;
        Ok(())
    }
}
