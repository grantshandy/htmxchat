use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};

use axum::{
    extract::{ws::Message, ConnectInfo, State, WebSocketUpgrade},
    http::Response,
    response::IntoResponse,
    routing::get,
    Router,
};
use fastrand::Rng;
use futures::{sink::SinkExt, stream::StreamExt};
use maud::{html, PreEscaped, DOCTYPE};
use tokio::sync::broadcast::{self, Receiver, Sender};

const SERVER_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 3030);
const MSG_CHANNEL_BOUND: usize = 1000;

const CSS: &str = r#"
.app { width: 90vw; margin-left: auto; margin-right: auto; border: 1px solid black; }
@media only screen and (min-width: 1076px) { .app { width: 40vw; } }
#submitbox { padding: 0.5rem; display: flex; align-items: center; border-top: 1px solid black; }
#submitform { flex-grow: 1; display: flex; height: 2rem; }
ul { overflow-y: auto; height: 50vh; list-style-type: none; margin: 0; padding: 1rem; }
li { margin-bottom: 0.5rem; }
h1 { text-align: center; }"#;

#[derive(serde::Deserialize)]
struct ClientMessage {
    pub msg: String,
}

#[tokio::main]
async fn main() {
    println!("starting server at http://{SERVER_ADDR}/");

    axum::Server::bind(&SERVER_ADDR)
        .serve(
            Router::new()
                .route("/", get(root))
                .route("/chat", get(chat))
                .with_state(Arc::new(broadcast::channel::<Message>(MSG_CHANNEL_BOUND)))
                .into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
}

async fn root() -> Response<String> {
    Response::builder()
        .header("Content-Type", "text/html")
        .body(
            html! {
                (DOCTYPE)
                html lang="en" {
                    head {
                        meta charset="UTF-8";
                        meta name="viewport" content="width=device-width, initial-scale=1.0";
                        script { (PreEscaped(include_str!("htmx.min.js"))) };
                        style { (CSS) }
                        title { "htmXchat" }
                    }
                    body {
                        h1 { "htmXchat" }
                        .app {
                            ul #messages {};
                            #submitbox hx-ws="connect:/chat" {};
                        }
                    }
                }
            }
            .into_string(),
        )
        .unwrap()
}

async fn chat(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<(Sender<Message>, Receiver<Message>)>>,
) -> impl IntoResponse {
    let client_tx = state.0.clone();

    ws.on_upgrade(move |mut socket| async move {
        if socket.send(Message::Ping(Vec::new())).await.is_err() {
            println!("{addr} could not connect");
            return;
        }

        if socket.recv().await.map(|msg| msg.is_err()).unwrap_or(false) {
            println!("client {addr} abruptly disconnected");
            return;
        }

        let (mut tx, mut rx) = socket.split();

        let recv_client_tx = client_tx.clone();
        let mut recv_task = tokio::spawn(async move {
            while let Some(Ok(msg)) = rx.next().await {
                match msg {
                    Message::Close(_) => break,
                    Message::Text(json) => {
                        if let Ok(ClientMessage { msg }) = serde_json::from_str(&json) {
                            if msg.split_whitespace().collect::<String>().is_empty() {
                                continue;
                            }

                            push_chat_msg(&recv_client_tx, &format!("> {msg}"), addr);
                        }
                    }
                    _ => (),
                }
            }
        });

        let mut client_rx = client_tx.clone().subscribe();
        let mut send_task = tokio::spawn(async move {
            while let Ok(msg) = client_rx.recv().await {
                tx.send(msg).await.unwrap();
            }
        });

        push_chat_msg(&client_tx, " joined.", addr);

        tokio::select! {
            _ = (&mut send_task) => recv_task.abort(),
            _ = (&mut recv_task) => send_task.abort(),
        }

        push_chat_msg(&client_tx, " left.", addr);
    })
}

fn push_chat_msg(client_tx: &Sender<Message>, msg: &str, addr: SocketAddr) {
    let mut rng = Rng::with_seed(
        match addr.ip() {
            IpAddr::V4(ip) => ip.octets().iter().map(|o| *o as u64).sum::<u64>(),
            IpAddr::V6(ip) => ip.octets().iter().map(|o| *o as u64).sum::<u64>(),
        } + (addr.port() as u64),
    );

    let user_color = format!("rgb({},{},{})", rng.u8(..200), rng.u8(..200), rng.u8(..200));

    client_tx
        .send(Message::Text(html! {
            div hx-swap-oob="beforeend:#messages" {
                li {
                    b style={ "color:" (user_color) } { (addr) }
                    (PreEscaped(msg))
                }
            }
            div hx-swap-oob="innerHTML:#submitbox" {
                form #submitform hx-ws="send:submit" {
                    input name="msg" type="text" placeholder="type your message!" autocomplete="off" style="flex-grow: 1;" autofocus;
                    input type="submit";
                }
                span style="margin-left: 2rem;" {
                    "You are "
                    b style={ "color:" (user_color) } { (addr)}
                }
            }
        }.into_string()))
        .unwrap();
}

// only 46 SLOC!
// this is the only file, all HTML is generated through a JSX-like template engine
