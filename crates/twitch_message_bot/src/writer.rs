use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use twitch_message::messages::{types::MsgId, Privmsg};

#[derive(Clone)]
pub struct Writer {
    sender: UnboundedSender<WriteKind>,
}

impl Writer {
    #[doc(hidden)]
    pub fn new() -> (Self, UnboundedReceiver<WriteKind>) {
        let (sender, rx) = tokio::sync::mpsc::unbounded_channel();
        (Self { sender }, rx)
    }
}

impl Writer {
    pub fn join_channel(&self, channel: impl ToString) {
        let _ = self.sender.send(WriteKind::Join {
            channel: channel.to_string().into(),
        });
    }

    pub fn part_channel(&self, channel: impl ToString) {
        let _ = self.sender.send(WriteKind::Part {
            channel: channel.to_string().into(),
        });
    }

    pub fn send_raw(&self, raw: impl ToString) {
        let _ = self.sender.send(WriteKind::Raw {
            raw: raw.to_string().into(),
        });
    }

    pub fn privmsg(&self, message: &Privmsg<'_>, data: impl ToString) {
        let _ = self.sender.send(WriteKind::Privmsg {
            target: message.channel.clone().into(),
            data: data.to_string().into(),
        });
    }

    pub fn reply(&self, message: &Privmsg<'_>, data: impl ToString) {
        let _ = self.sender.send(WriteKind::Reply {
            id: message.msg_id().expect("msg-id attached").to_owned(),
            target: message.channel.clone().into(),
            data: data.to_string().into(),
        });
    }

    pub fn quit(&self) {
        let _ = self.sender.send(WriteKind::Quit);
    }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum WriteKind {
    Join {
        channel: Box<str>,
    },
    Part {
        channel: Box<str>,
    },
    Raw {
        raw: Box<str>,
    },
    Privmsg {
        target: Box<str>,
        data: Box<str>,
    },
    Reply {
        id: MsgId,
        target: Box<str>,
        data: Box<str>,
    },
    Quit,
}

impl std::fmt::Display for WriteKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use twitch_message::encode::*;
        match self {
            Self::Join { channel } => join(channel).format(f),
            Self::Part { channel } => part(channel).format(f),
            Self::Raw { raw: msg } => raw(msg).format(f),
            Self::Privmsg { target, data } => privmsg(target, data).format(f),
            Self::Reply { id, target, data } => reply(id, target, data).format(f),
            Self::Quit => f.write_str("QUIT\r\n"),
        }
    }
}
