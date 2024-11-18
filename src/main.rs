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

// #[get("/redis/sub/{channel}")]
// async fn subscribe_channel(
//     data: web::Data<AppState>,
//     channel: web::Path<String>,
// ) -> impl Responder {
//     let client = data.redis_client.clone();
//     let mut connection = match client.get_connection() {
//         Ok(conn) => conn,
//         Err(_) => return HttpResponse::InternalServerError().body("Failed to connect to Redis"),
//     };

//     let channel_name = channel.into_inner();
//     let mut pubsub = connection.as_pubsub();
//     match pubsub.subscribe(&channel_name) {
//         Ok(_) => (),
//         Err(_) => return HttpResponse::InternalServerError().body("Failed to subscribe to channel"),
//     }

//     let stream = unfold(pubsub, |mut pubsub| async {
//         let msg = pubsub.get_message().ok();
//         let message = msg
//             .as_ref()
//             .map(|m| m.get_payload::<String>().unwrap_or_else(|_| "Invalid UTF-8 payload".to_string()));
//         match message {
//             Some(m) => Some((Ok::<_, actix_web::Error>(web::Bytes::from(m)), pubsub)),
//             None => None,
//         }
//     });

//     HttpResponse::Ok()
//         .content_type("text/plain")
//         .streaming(stream)
// }


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
            // .service(subscribe_channel)
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
