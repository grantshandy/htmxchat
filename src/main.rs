use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};

use axum::{
    extract::{
        ws::{self, Message},
        ConnectInfo, State, WebSocketUpgrade,
    },
    http::Response,
    response::IntoResponse,
    routing::get,
    Router,
};
use fastrand::Rng;
use futures::{sink::SinkExt, stream::StreamExt};
use maud::{html, Markup, PreEscaped, Render, DOCTYPE};
use serde_json::Value;
use tokio::sync::{
    broadcast::{self, Receiver, Sender},
    Mutex,
};

#[tokio::main]
async fn main() {
    let socket: SocketAddr = "127.0.0.1:3030".parse().unwrap();

    println!("starting server at http://{socket}/");

    axum::Server::bind(&socket)
        .serve(
            Router::new()
                .route("/", get(root))
                .route("/chat", get(chat))
                .with_state(Arc::new(ClientPool::default()))
                .into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
}

async fn root() -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                script { (PreEscaped(include_str!("../assets/htmx.min.js"))) };
                style { (PreEscaped(include_str!(concat!(env!("OUT_DIR"), "/output.css")))) }
                title { "htmXchat" }
            }
            body {
                div class="w-5/6 md:w-1/2 mx-auto p-4 space-y-2" {
                    h1 class="text-center text-2xl font-bold" { "htmXchat" }
                    div class="border rounded-md p-2 space-y-2" {
                        ul #messages class="border rounded-md" {};
                        #messagebox class="flex items-center space-x-2 p-2 border rounded-md" hx-ws="connect:/chat" {};
                    }
                }
            }
        }
    }
}

fn messagebox(addr: SocketAddr) -> Message {
    Message::Text(html! {
        div hx-swap-oob="innerHTML:#messagebox" {
            form hx-ws="send:submit" class="flex-grow flex" {
                input name="msg" type="text" placeholder="type your message!" autocomplete="off" class="flex-grow px-2" autofocus;
                input type="submit" class="bg-blue-500 rounded-r-md text-white px-2";
            }
            span class="flex-grow" {
                "You are " b style=(addr_to_css_color(addr)) { (addr) }
            }
        }
    }.into_string())
}


async fn chat(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(client_pool): State<Arc<ClientPool>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |mut socket| async move {
        if socket.send(Message::Ping(Vec::new())).await.is_err() {
            client_pool.send(addr, "<failed to connect>");
            return;
        }

        if socket.recv().await.map(|msg| msg.is_err()).unwrap_or(false) {
            return;
        }

        socket.send(messagebox(addr)).await.unwrap();

        let (tx, mut rx) = socket.split();
        let tx = Arc::new(Mutex::new(tx));

        let client_pool_tx = client_pool.clone();
        let mut submitbox_tx = tx.clone();
        let mut recv_task = tokio::spawn(async move {
            while let Some(Ok(msg)) = rx.next().await {
                match msg {
                    Message::Text(text) => {
                        let msg = serde_json::from_str::<Value>(text.as_str()).unwrap_or_default()
                            ["msg"]
                            .to_string();
                        let mut chars = msg.chars();
                        chars.next();
                        chars.next_back();

                        let text: String = chars.collect();

                        if !text.is_empty() {
                            client_pool_tx.send(addr, &text);
                            submitbox_tx.lock().await.send(messagebox(addr)).await.unwrap();
                        }
                    }
                    Message::Close(_) => break,
                    _ => (),
                }
            }
        });

        let mut client_pool_rx = client_pool.clone().subscribe();
        let mut send_task = tokio::spawn(async move {
            while let Ok(msg) = client_pool_rx.recv().await {
                tx.lock().await.send(msg).await.unwrap();
            }
        });

        client_pool.send(addr, "<JOINED>");

        tokio::select! {
            _ = (&mut send_task) => recv_task.abort(),
            _ = (&mut recv_task) => send_task.abort(),
        }

        client_pool.send(addr, "<LEFT>");
    })
}

struct ClientPool {
    tx: Sender<Message>,
    _rx: Receiver<Message>,
}

impl Default for ClientPool {
    fn default() -> Self {
        let (tx, _rx) = broadcast::channel(1000);

        Self { tx, _rx }
    }
}

impl ClientPool {
    pub fn send(&self, user: SocketAddr, message: &str) {
        let color = addr_to_css_color(user);

        self.tx
            .send(Message::Text(
                html! {
                    div hx-swap-oob="beforeend:#messages" {
                        li style=(color) { (user) "> " (message) }
                    }
                }
                .into_string(),
            ))
            .unwrap();

        println!("{user}> {message}");
    }

    pub fn subscribe(&self) -> Receiver<Message> {
        self.tx.subscribe()
    }
}

fn addr_to_css_color(addr: SocketAddr) -> String {
    let mut rng = Rng::with_seed(
        addr.ip()
            .to_string()
            .as_bytes()
            .iter()
            .map(|o| *o as u64)
            .sum::<u64>()
            + (addr.port() as u64),
    );

    format!(
        "color: rgb({},{},{});",
        rng.u8(..200),
        rng.u8(..200),
        rng.u8(..200)
    )
}
