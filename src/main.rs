mod discord;
mod slack;

mod commands;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    tokio::join!(
        discord::start(),
        slack::start()
    );
}
