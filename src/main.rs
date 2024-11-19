use actix_web::{get, post, web, App, Error, HttpMessage, HttpRequest, HttpResponse, HttpServer, Responder};
use actix_web::web::Data;
use actix_web::http::{StatusCode, header};
use std::io;
use std::sync::Mutex;
use redis::{Commands, Client};
use tokio_stream::{self as stream, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::fs::read_to_string;
use futures_util::stream::unfold;
use futures_util::stream::poll_fn;
use actix_web::web::Bytes;
use std::task::{Context, Poll};

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
    req: HttpRequest, // Include the request to monitor connection status
) -> impl Responder {
    let channel_name = channel.into_inner();
    println!("Subscribing to channel via SSE: {}", channel_name);
    let client = data.redis_client.clone();
    let mut counter: usize = 5;

    println!("Attempting to get Redis connection...");
    let mut connection = match client.get_connection() {
        Ok(conn) => conn,
        Err(e) => {
            println!("Failed to obtain Redis connection: {:?}", e);
            // return Poll::Ready(None);
            return  HttpResponse::NotFound().body("Not found Redis connection");
        }
    };

    // Create a `poll_fn`-based stream
    let server_events = poll_fn(move |cx: &mut Context<'_>| -> Poll<Option<Result<Bytes, Error>>> {
        if counter == 0 {
            return Poll::Ready(None);
        }
        let payload = format!("data: {}\n\n", counter);
        counter -= 1;

        let mut pubsub = connection.as_pubsub();
        println!("Subscribing to channel: {}", channel_name);
        if let Err(e) = pubsub.subscribe(&channel_name) {
            println!("Failed to subscribe to channel: {:?}", e);
            return Poll::Ready(None);
        }

        // Try to get a message; this simulates the non-blocking behavior
        let msg = match pubsub.get_message() {
            Ok(message) => Some(message),
            Err(_) => None, // Returning None on error ends the stream
        };

        if let Some(message) = msg {
            let payload = match message.get_payload::<String>() {
                Ok(payload) => payload,
                Err(_) => "Invalid UTF-8 payload".to_string(),
            };
            println!("Message content: {}", payload);
            let event = format!("data: {}\n\n", payload);
            return Poll::Ready(Some(Ok(Bytes::from(event))));
        }

        // If no message, return Pending to keep the connection open
        cx.waker().wake_by_ref(); // Ensure we are polled again
        Poll::Pending
    });


    HttpResponse::Ok()
    .content_type("text/event-stream")
    .append_header(("content-encoding","identity"))
    .streaming(server_events)

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
