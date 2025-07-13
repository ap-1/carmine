mod discord;
mod slack;

mod commands;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    discord::start().await;
    slack::start().await;
}
