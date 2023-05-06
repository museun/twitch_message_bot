// #![cfg_attr(debug_assertions, allow(dead_code, unused_variables,))]
use std::time::Duration;

use twitch_message::messages::{Message, Privmsg};

// struct Bot;

// #[async_trait::async_trait]
// impl Handler for Bot {
//     fn init() -> Result<Self, crate::Error> {
//         Ok(Self)
//     }

//     async fn on_connected<'a>(&'a mut self, identity: Identity, writer: Writer) {
//         writer.join_channel("museun")
//     }

//     async fn on_privmsg<'a>(&'a mut self, message: Privmsg<'static>, writer: Writer) {
//         eprintln!("{} {}: {}", message.channel, message.sender, message.data);
//     }
// }

// #[tokio::main]
// async fn main() {
//     Bot::connect(Config::new("shaken_bot", "hunter2"))
//         .await
//         .unwrap();
// }

#[async_trait::async_trait]
pub trait Handler: Send + Sync + 'static {
    async fn init() -> Result<Self, crate::Error>
    where
        Self: Sized;

    async fn connect(config: Config) -> Result<(), Error>
    where
        Self: Sized,
    {
        client::connect::<Self>(config).await
    }

    async fn on_connected<'a>(&'a mut self, identity: Identity, writer: Writer);
    async fn on_connecting<'a>(&'a mut self) {}
    async fn on_disconnected<'a>(&'a mut self, error: Error) -> Reconnect {
        let _error = error;
        Reconnect::Always
    }

    async fn on_twitch_message<'a>(&'a mut self, _message: Message<'static>, writer: Writer) {
        let _writer = writer;
    }
    async fn on_privmsg<'a>(&'a mut self, message: Privmsg<'static>, writer: Writer);
    async fn on_join<'a, 'b>(&'a mut self, channel: &'b str) {
        let _channel = channel;
    }
    async fn on_part<'a, 'b>(&'a mut self, channel: &'b str) {
        let _channel = channel;
    }
}

pub enum Reconnect {
    Always,
    Never,
    After(Duration),
}

mod config;
pub use config::Config;

mod writer;
#[doc(hidden)]
pub use writer::WriteKind;
pub use writer::Writer;

mod client;
pub use client::{Error, Identity};

mod util;

/// Re-exports
pub use async_trait::async_trait;
pub use twitch_message;
