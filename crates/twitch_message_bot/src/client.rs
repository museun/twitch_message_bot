use std::{
    collections::{HashSet, VecDeque},
    time::Duration,
};

use tokio::{
    io::{AsyncWrite, AsyncWriteExt},
    net::TcpStream,
    sync::mpsc::UnboundedReceiver,
};
use twitch_message::{
    encode::{Encodable, ALL_CAPABILITIES},
    messages::{Privmsg, TwitchMessage},
    Badge, Color, IntoStatic, PingTracker,
};

use crate::{writer::WriteKind, Config, Handler, Reconnect, Writer};

#[non_exhaustive]
#[derive(Debug)]
pub enum Error {
    CannotWrite,
    CannotRegister,
    CannotRead,
    CannotInit {
        error: Box<dyn std::error::Error + Send + Sync>,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let err = match self {
            Self::CannotWrite => "Cannot write to the socket",
            Self::CannotRegister => "Cannot register with the IRC server",
            Self::CannotRead => "Cannot read from the socket",
            Self::CannotInit { error } => return write!(f, "Cannot initialize handler: {error}"),
        };
        f.write_str(err)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CannotInit { error } => Some(&**error),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct Identity {
    pub user_id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub color: Option<Color>,
    pub emote_sets: Vec<String>,
    pub global_badges: Vec<twitch_message::Badge<'static>>,
}

pub struct Client<'a, H> {
    pub(crate) handler: H,
    pub(crate) buf: Vec<u8>,

    recv: UnboundedReceiver<WriteKind>,
    writer: Writer,
    channels: HashSet<Box<str>>,
    queue: VecDeque<WriteKind>,
    config: &'a Config,
}

impl<'a, H: Handler> Client<'a, H> {
    pub fn new(
        handler: H,
        recv: UnboundedReceiver<WriteKind>,
        writer: Writer,
        config: &'a Config,
    ) -> Self {
        Self {
            handler,
            recv,
            writer,
            channels: HashSet::new(),
            queue: VecDeque::new(),
            buf: Vec::with_capacity(1024),
            config,
        }
    }

    pub async fn connect(config: &Config, buf: &mut Vec<u8>) -> Result<TcpStream, Error> {
        let Ok(mut conn) = TcpStream::connect(twitch_message::TWITCH_IRC_ADDRESS).await else {
            return Err(Error::CannotWrite);
        };

        let register = twitch_message::encode::register(
            &config.name, //
            &config.token,
            ALL_CAPABILITIES,
        );

        if let Err(..) = Self::write(&mut conn, register, buf).await {
            return Err(Error::CannotRegister);
        }

        Ok(conn)
    }

    pub async fn run(&mut self, mut conn: TcpStream) -> Result<(), Error> {
        static TOKEN: &str = concat!(env!("CARGO_PKG_NAME"), "+", env!("CARGO_PKG_VERSION"));

        use crate::util::Either::*;
        use tokio::io::AsyncBufReadExt as _;

        let (read, mut write) = conn.split();
        let mut read = tokio::io::BufReader::new(read).lines();
        let pt = PingTracker::new(self.config.ping_delay);
        let mut our_name = <Option<String>>::None;

        for channel in &self.channels {
            let msg = twitch_message::encode::join(channel);
            log::debug!("rejoining: {channel}");
            Self::write(&mut write, msg, &mut self.buf).await?;
        }

        // TODO make 'quit' a signal, not a message in the queue
        loop {
            let should_pong = pt.should_pong();
            if let Some(pong) = should_pong {
                if Self::write(&mut write, pong, &mut self.buf).await.is_err() {
                    return Err(Error::CannotWrite);
                }
            }

            let left = async {
                let data = read.next_line().await.ok().flatten()?;
                twitch_message::parse(&data)
                    .map(|p| p.message.into_static())
                    .ok()
            };
            let mut left = std::pin::pin!(left);

            let right = self.recv.recv();
            let mut right = std::pin::pin!(right);

            let event = match tokio::time::timeout(
                self.config.ping_delay,
                crate::util::select2(&mut left, &mut right),
            )
            .await
            {
                Ok(val) => val,
                Err(_) => {
                    log::warn!(
                        "no data sent or received in {:?}. sending a ping",
                        self.config.ping_delay
                    );

                    let msg = twitch_message::encode::ping(TOKEN);
                    if Self::write(&mut write, msg, &mut self.buf).await.is_err() {
                        return Err(Error::CannotWrite);
                    }

                    continue;
                }
            };

            match event {
                Left(Some(msg)) => {
                    pt.update(&msg);

                    match msg.as_enum() {
                        TwitchMessage::Ready(msg) => {
                            our_name.replace(msg.name.to_string());
                        }

                        TwitchMessage::GlobalUserState(msg) => {
                            let identity = Identity {
                                user_id: msg
                                    .user_id()
                                    .expect("our user must have an id")
                                    .to_string(),
                                name: our_name.clone().expect("name state must be valid"),
                                display_name: msg.display_name().map(ToString::to_string),
                                color: msg.color(),
                                emote_sets: msg.emote_sets().map(ToString::to_string).collect(),
                                global_badges: msg
                                    .badge_info()
                                    .map(|Badge { name, version }| Badge {
                                        name: name.into_static(),
                                        version: version.into_static(),
                                    })
                                    .collect(),
                            };
                            self.handler
                                .on_connected(identity, self.writer.clone())
                                .await;

                            while let Some(msg) = self.queue.pop_front() {
                                Self::handle_write(&mut write, &msg, &mut self.buf).await?;
                                if matches!(msg, WriteKind::Quit) {
                                    return Ok(());
                                }
                            }
                        }

                        TwitchMessage::Join(msg) if our_name.as_deref() == Some(&*msg.user) => {
                            self.handler.on_join(&msg.channel).await;
                        }

                        TwitchMessage::Part(msg) if our_name.as_deref() == Some(&*msg.user) => {
                            self.handler.on_part(&msg.channel).await;
                        }

                        _ => {}
                    }

                    if let Some(pm) = msg.as_typed_message::<Privmsg>() {
                        self.handler
                            .on_privmsg(pm.clone(), self.writer.clone())
                            .await;
                    };

                    self.handler
                        .on_twitch_message(msg, self.writer.clone())
                        .await;
                }

                Right(Some(kind)) if our_name.is_some() => {
                    Self::handle_write(&mut write, &kind, &mut self.buf).await?;
                    if matches!(kind, WriteKind::Quit) {
                        return Ok(());
                    }
                }

                Right(Some(kind)) => self.queue.push_back(kind),

                Left(None) => {
                    log::warn!("cannot read from connection");
                    return Err(Error::CannotRead);
                }

                Right(None) => {
                    log::warn!("cannot read from shared 'writer'");
                    return Ok(());
                }
            }
        }
    }

    pub fn drain_pending_writes(&mut self) {
        while let Ok(msg) = self.recv.try_recv() {
            match msg {
                WriteKind::Join { channel } => {
                    let _ = self.channels.insert(channel);
                }
                WriteKind::Part { channel } => {
                    let _ = self.channels.remove(&*channel);
                }
                msg => self.queue.push_back(msg),
            }
        }
    }

    async fn write(
        io: &mut (impl AsyncWrite + Send + Unpin),
        msg: impl Encodable + Send,
        mut buf: &mut Vec<u8>,
    ) -> Result<(), Error> {
        buf.clear();
        msg.encode(&mut buf).map_err(|_| Error::CannotWrite)?;
        io.write_all(&*buf).await.map_err(|_| Error::CannotWrite)?;
        io.flush().await.map_err(|_| Error::CannotWrite)
    }

    async fn handle_write(
        conn: &mut (impl AsyncWrite + Send + Unpin),
        kind: &WriteKind,
        buf: &mut Vec<u8>,
    ) -> Result<(), Error> {
        use twitch_message::encode::{join, part, privmsg, raw, reply};
        use WriteKind::*;

        match kind {
            Join { channel } => Self::write(conn, join(channel), buf).await,
            Part { channel } => Self::write(conn, part(channel), buf).await,
            Raw { raw: msg } => Self::write(conn, raw(msg), buf).await,
            Privmsg { target, data } => {
                for part in data.split('\n') {
                    Self::write(conn, privmsg(target, part.trim()), buf).await?;
                }
                Ok(())
            }
            Reply { id, target, data } => {
                for part in data.split('\n') {
                    Self::write(conn, reply(id, target, part.trim()), buf).await?;
                }
                Ok(())
            }
            Quit => Self::write(conn, QuitMessage, buf).await,
        }
    }
}

struct QuitMessage;

impl std::fmt::Display for QuitMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("QUIT\r\n")
    }
}

impl twitch_message::encode::Encodable for QuitMessage {
    fn encode(&self, mut writer: impl std::io::Write) -> std::io::Result<()> {
        write!(&mut writer, "QUIT\r\n")
    }
}
impl twitch_message::encode::Formattable for QuitMessage {
    fn format(&self, mut fmt: impl std::fmt::Write) -> core::fmt::Result {
        write!(&mut fmt, "QUIT\r\n")
    }
}

pub async fn connect<H: Handler>(config: Config) -> Result<(), crate::Error> {
    const DEFAULT_DELAY: Duration = Duration::from_secs(10);

    let (writer, recv) = Writer::new();

    let handler = H::init().await?;
    let mut client = Client::new(handler, recv, writer, &config);

    loop {
        client.handler.on_connecting().await;

        let conn = match Client::<H>::connect(&config, &mut client.buf).await {
            Ok(conn) => conn,
            Err(error) => {
                let delay = match client.handler.on_disconnected(error).await {
                    Reconnect::Never => break,
                    Reconnect::Always => DEFAULT_DELAY,
                    Reconnect::After(delay) => delay,
                };
                log::debug!("waiting: {delay:.2?} to reconnect");
                tokio::time::sleep(delay).await;
                continue;
            }
        };

        client.drain_pending_writes();

        match client.run(conn).await {
            Ok(..) => break,
            Err(error) => {
                let delay = match client.handler.on_disconnected(error).await {
                    Reconnect::Never => break,
                    Reconnect::Always => DEFAULT_DELAY,
                    Reconnect::After(delay) => delay,
                };
                log::debug!("waiting: {delay:.2?} to reconnect");
                tokio::time::sleep(delay).await;
            }
        }
    }

    Ok(())
}
