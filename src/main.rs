use actix_web::{web, App, HttpServer, HttpResponse, Responder, get, post, Error};
use actix_web::web::Data;
use std::io;
use std::sync::Mutex;
use redis::{Commands, Client};
use tokio_stream::{self as stream, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::fs::read_to_string;
use futures_util::stream::unfold;
use actix_web::web::Bytes;

#[derive(Deserialize)]
struct PubSubMessage {
    channel: String,
    message: String,
}

struct AppState {
    redis_client: redis::Client,
}

#[get("/")]
async fn get_homepage() -> Result<impl Responder, Error> {
    match read_to_string("./gui.html").await {
        Ok(content) => Ok(HttpResponse::Ok().content_type("text/html").body(content)),
        Err(_) => Ok(HttpResponse::NotFound().body("Not found gui.html")),
    }
}

#[post("/redis/pub/")]
async fn publish_message(data: web::Data<AppState>, msg: web::Json<PubSubMessage>) -> impl Responder {
    let mut conn = data.redis_client.get_connection().unwrap();
    let _: () = conn.publish(&msg.channel, &msg.message).unwrap();
    HttpResponse::Ok().json(format!("Message published to channel '{}'", msg.channel))
}

#[get("/redis/sub/{channel}")]
async fn subscribe_channel(
    data: web::Data<AppState>,
    channel: web::Path<String>,
) -> impl Responder {
    let channel_name = channel.into_inner();
    println!("Subscribing to channel: {}", channel_name);
    let client = data.redis_client.clone();
    println!("Cloned Redis client");

    let stream = unfold((channel_name, client), |(channel_name, client)| async {
        println!("Attempting to get Redis connection...");
        let mut connection = match client.get_connection() {
            Ok(conn) => {
                println!("Successfully obtained Redis connection");
                conn
            },
            Err(e) => {
                println!("Failed to obtain Redis connection: {:?}", e);
                return None;
            }
        };

        let mut pubsub = connection.as_pubsub();
        println!("Attempting to subscribe to channel: {}", channel_name);
        match pubsub.subscribe(&channel_name) {
            Ok(_) => println!("Successfully subscribed to channel: {}", channel_name),
            Err(e) => {
                println!("Failed to subscribe to channel: {:?} - Continuing", e);
            }
        }

        println!("Waiting for a message...");
        let msg = pubsub.get_message().ok();
        if let Some(ref message) = msg {
            println!("Received a message");
        } else {
            println!("No message received or an error occurred");
        }

        let message = msg
            .as_ref()
            .map(|m| m.get_payload::<String>().unwrap_or_else(|_| {
                println!("Failed to decode message payload");
                "Invalid UTF-8 payload".to_string()
            }));
        
        match message {
            Some(m) => {
                println!("Message content: {}", m);
                // Convert message to web::Bytes and immediately flush it
                let event = format!("data: {}\n\n", m);
                Some((Ok::<_, actix_web::Error>(web::Bytes::from(event)), (channel_name, client)))
            },
            None => {
                println!("No valid message to process");
                None
            }
        }
    });

    HttpResponse::Ok()
        .content_type("text/event-stream")
        // .no_chunking() // Ensures immediate delivery of each message
        .streaming(stream)
}



#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let redis_client = redis::Client::open("redis://127.0.0.1/").unwrap();

    HttpServer::new(move || {
        App::new()
            .app_data(Data::new(AppState {
                redis_client: redis_client.clone(),
            }))
            .service(get_homepage)
            .service(publish_message)
            .service(subscribe_channel)
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
