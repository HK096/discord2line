// #![windows_subsystem = "windows"] //コマンドプロンプトを表示しない
use dotenv::dotenv;
use reqwest::Client as HttpClient;
use serde::Deserialize;
use serenity::all::ChannelId;
use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

struct Handler {
    last_notification_time: Arc<Mutex<HashMap<ChannelId, Instant>>>,
    notification_interval: Duration,
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.bot {
            return;
        }
        if msg.guild_id.is_none() {
            return;
        }
        if msg.content.is_empty() {
            return;
        }

        let mut last_notification_time = self.last_notification_time.lock().await;
        let channel_id = msg.channel_id;

        let should_notify = match last_notification_time.get(&channel_id) {
            Some(&last_time) => last_time.elapsed() >= self.notification_interval,
            None => true,
        };

        if should_notify {
            let link = msg.link();
            let channel_id = msg.channel_id;
            let channel_name = match channel_id.to_channel(&ctx.http).await {
                Ok(channel) => channel.guild().map(|c| c.name).unwrap_or_default(),
                Err(_) => String::from("Unknown channel"),
            };
            let content = msg.content;

            let message_url = match shorten(&link).await {
                Ok(shortened) => shortened,
                Err(_) => link.clone(),
            };

            let mut nick_name = msg.author.name.clone();
            if let Some(guild_id) = msg.guild_id {
                if let Ok(member) = guild_id.member(&ctx.http, msg.author.id).await {
                    if let Some(nick) = member.nick {
                        nick_name = nick;
                    }
                }
            }
            let response = format!(
                "\n{}で{}からの発言\n--------\n{}\n-------- {}",
                channel_name, nick_name, content, message_url
            );

            get_msg(response).await;

            last_notification_time.insert(channel_id, Instant::now());
        }
    }
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

async fn shorten(url: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let api_key = env::var("XGD_API_KEY").expect("Expected an API key in the environment");
    let api_url = format!("https://xgd.io/V1/shorten?url={}&key={}", url, api_key);

    let http_client = HttpClient::new();

    let response = http_client.get(&api_url).send().await?;

    if response.status().is_success() {
        let response_body = response.text().await?;
        let response_data: XgdResponse = serde_json::from_str(&response_body)?;
        Ok(response_data.shorturl)
    } else {
        // 短縮に失敗した場合は元のURLを返す
        Ok::<String, _>(url.to_string())
    }
}

#[derive(Deserialize)]
struct XgdResponse {
    shorturl: String,
}

async fn get_msg(msg: String) {
    if let Err(e) = send(msg).await {
        println!("error sending message: {}", e);
    }
}

async fn send(msg: String) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use reqwest::{header, Client};

    let mut notify_token = env::var("NOTIFY_TOKEN").expect("Failed getting NOTIFY_TOKEN");
    notify_token = format!("Bearer {}", notify_token);
    let url = "https://notify-api.line.me/api/notify";

    let mut message = HashMap::new();
    message.insert("message", msg);

    let mut head = header::HeaderMap::new();
    let token = header::HeaderValue::from_str(&notify_token)?;
    head.insert("Authorization", token);

    let client = Client::new();
    match client.post(url).headers(head).form(&message).send().await {
        Ok(res) => {
            println!("Status is {:?}", res.status());
        }
        Err(e) => {
            println!("Error sending notification: {:?}", e);
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    dotenv().ok();
    let discord_token = env::var("DISCORD_TOKEN").expect("Failed getting DISCORD_TOKEN ");
    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let handler = Handler {
        last_notification_time: Arc::new(Mutex::new(HashMap::new())),
        notification_interval: Duration::from_secs(60),
    };

    let mut client = Client::builder(discord_token, intents)
        .event_handler(handler)
        .await
        .expect("Err creating client");

    if let Err(why) = client.start().await {
        println!("Client error: {why:?}");
    }
}
