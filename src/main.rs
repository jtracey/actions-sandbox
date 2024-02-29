use anyhow::Result;
use futures_util::{stream::SplitStream, SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteConnectOptions, ConnectOptions};
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::str::FromStr;
use tokio::io::{stdin, AsyncBufReadExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_stream::{
    wrappers::{errors::BroadcastStreamRecvError, BroadcastStream, LinesStream},
    Stream,
};
use tokio_tungstenite::{accept_async, tungstenite, tungstenite::Error, WebSocketStream};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Message {
    timestamp: u64,
    body: String,
}

#[derive(Serialize)]
struct FrontendMessage {
    message: Message,
    stale: bool,
}

async fn accept_connection<
    T: Stream<Item = Result<Message, BroadcastStreamRecvError>> + std::marker::Unpin,
>(
    peer: SocketAddr,
    stream: TcpStream,
    message_channel: T,
) {
    if let Err(e) = handle_connection(peer, stream, message_channel).await {
        match e {
            Error::ConnectionClosed | Error::Protocol(_) | Error::Utf8 => (),
            err => eprintln!("Error processing connection: {}", err),
        }
    }
}

async fn handle_connection<
    T: Stream<Item = Result<Message, BroadcastStreamRecvError>> + std::marker::Unpin,
>(
    peer: SocketAddr,
    stream: TcpStream,
    mut message_channel: T,
) -> tokio_tungstenite::tungstenite::Result<()> {
    let ws_stream = accept_async(stream).await.expect("Failed to accept");

    println!("New WebSocket connection: {}", peer);
    let (mut write, read) = ws_stream.split();

    tokio::spawn(get_commands(read));

    let mut window = VecDeque::<Message>::new();
    while let Some(msg) = message_channel.next().await {
        let msg = msg.unwrap();
        let stale = if window.len() < 100 {
            window.push_back(msg.clone());
            false
        } else if msg.timestamp > window.front().unwrap().timestamp {
            window.push_back(msg.clone());
            let _ = window.pop_front();
            false
        } else {
            true
        };
        let outgoing = FrontendMessage {
            message: msg,
            stale,
        };
        let outgoing = serde_json::to_string(&outgoing).unwrap();
        let outgoing = tungstenite::Message::Text(outgoing);
        write.send(outgoing).await?;
    }

    Ok(())
}

#[derive(Deserialize)]
struct Command {
    // for now, only one command: get messages around timestamp
    timestamp: u64,
}

async fn get_commands(mut read: SplitStream<WebSocketStream<TcpStream>>) {
    loop {
        let ws_message = read.next().await.unwrap().unwrap();
        let command: Command = serde_json::from_str(&ws_message.into_text().unwrap()).unwrap();
        todo!("{}", command.timestamp);
    }
}

async fn get_messages<T: Stream<Item = Result<String, std::io::Error>> + std::marker::Unpin>(
    mut channel_in: T,
    channel_out: broadcast::Sender<Message>,
) {
    loop {
        let message_in = channel_in.next().await;
        let Some(message_in) = message_in else {
            return;
        };
        let message_in = message_in.unwrap();
        let message: Message = serde_json::from_str(&message_in).unwrap();
        channel_out.send(message).unwrap();
    }
}

async fn run<
    T: Stream<Item = Result<String, std::io::Error>>
        + std::marker::Send
        + std::marker::Unpin
        + 'static,
>(
    message_channel: T,
) -> Result<()> {
    let _conn = SqliteConnectOptions::from_str("sqlite://messages.db")?
        .create_if_missing(true)
        .connect()
        .await?;

    let addr = "127.0.0.1:9002";
    let listener = TcpListener::bind(&addr).await.expect("Can't listen");
    println!("Listening on: {}", addr);

    let (send, _receive) = broadcast::channel::<Message>(1000);
    let send2 = send.clone();
    tokio::spawn(get_messages(message_channel, send2));

    while let Ok((stream, _)) = listener.accept().await {
        let peer = stream
            .peer_addr()
            .expect("connected streams should have a peer address");
        println!("Peer address: {}", peer);
        tokio::spawn(accept_connection(
            peer,
            stream,
            BroadcastStream::new(send.subscribe()),
        ));
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let stdin = BufReader::new(stdin());
    let lines = LinesStream::new(stdin.lines());
    run(lines).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{Rng, SeedableRng};

    struct MessageCreator {
        current_time: u64,
        rng: rand::rngs::StdRng,
        queue: Option<(u64, u32)>, // timestamp and remaining run
    }

    impl MessageCreator {
        fn new() -> Self {
            MessageCreator {
                current_time: 2_u64.pow(32),
                rng: rand::rngs::StdRng::from_seed([0; 32]),
                queue: None,
            }
        }
    }

    impl Iterator for MessageCreator {
        type Item = Message;

        fn next(&mut self) -> Option<Self::Item> {
            self.current_time += 1;
            if let Some(mut queue) = self.queue {
                queue.0 += 1;
                queue.1 -= 1;
                if queue.1 == 0 {
                    self.queue = None;
                }
                return Some(Message {
                    timestamp: queue.0,
                    body: "".to_string(),
                });
            }

            let prob_of_late_message = 0.1;
            let timestamp = if self.rng.gen_bool(prob_of_late_message) {
                let timestamp = self.rng.gen_range(0..self.current_time);

                let prob_of_run = 0.1;
                if self.rng.gen_bool(prob_of_run) {
                    self.queue = Some((timestamp, self.rng.gen_range(2..20)));
                }
                timestamp
            } else {
                self.current_time
            };
            Some(Message {
                timestamp,
                body: "".to_string(),
            })
        }
    }

    #[test]
    fn generate_messages() {
        let mut creator = MessageCreator::new();
        for _ in 0..1000 {
            creator.next();
        }
    }

    #[tokio::test]
    async fn runs() {
        let stream = tokio_stream::iter(
            MessageCreator::new()
                .take(200)
                .map(|s| Ok(serde_json::to_string(&s).unwrap())),
        );
        tokio::time::timeout(std::time::Duration::from_secs(5), run(stream))
            .await
            .expect_err("finished early, should still be waiting for websocket connections");
    }
}
