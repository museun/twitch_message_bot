use std::{future::Future, sync::Arc};

use tokio::sync::mpsc::UnboundedReceiver;
use twitch_message::{builders::TagsBuilder, messages::Privmsg};
use twitch_message_bot::{WriteKind, Writer};

use crate::{
    bind::{BindOptions, BoxFuture, Callable},
    Bind, Command, Context, Dispatcher, Outcome,
};

pub trait MockContext<T>: Sized + Send + Sync + 'static
where
    T: Send + Sync + 'static,
{
    fn mock(self, this: T, cmd: Command, opts: BindOptions) -> MockBinding;
}

impl<F, T, Fut, O> MockContext<T> for F
where
    F: Fn(Arc<tokio::sync::Mutex<T>>, Context) -> Fut,
    F: Send + Sync + Copy + 'static,
    T: Send + Sync + 'static,
    Fut: Future<Output = O> + Send + Sync + 'static,
    O: Outcome,
{
    fn mock(self, this: T, cmd: Command, opts: BindOptions) -> MockBinding {
        Bind::create(this)
            .bind::<_, Fut, O>(cmd, self, opts)
            .finish()
            .mock()
    }
}

pub trait MockHandler: Sized {
    fn mock(self) -> MockBinding;
}

impl MockHandler for Arc<Callable> {
    fn mock(self) -> MockBinding {
        let inner = Box::new({
            move |msg, writer| -> BoxFuture<'static> {
                let this = Arc::clone(&self);
                Box::pin((this)(msg, writer))
            }
        });
        let (writer, recv) = Writer::new();

        MockBinding {
            recv,
            writer,
            inner,
        }
    }
}

impl MockHandler for Dispatcher {
    fn mock(self) -> MockBinding {
        let inner = Box::new(move |msg, writer| -> BoxFuture<'static> {
            let this = self.clone();
            Box::pin(async move { this.dispatch_async(msg, writer).await })
        });
        let (writer, recv) = Writer::new();

        MockBinding {
            inner,
            writer,
            recv,
        }
    }
}

pub struct MockBinding {
    recv: UnboundedReceiver<WriteKind>,
    writer: Writer,
    inner: Box<dyn Fn(Arc<Privmsg<'static>>, Writer) -> BoxFuture<'static> + Send + Sync + 'static>,
}

impl MockBinding {
    pub async fn send_privmsg(&self, pm: Privmsg<'static>) {
        (self.inner)(Arc::new(pm), self.writer.clone()).await
    }

    pub fn privmsg_builder<'a, 'e>(&'a self, data: &'e str) -> SendGuard<'a, 'e> {
        SendGuard {
            inner: self,
            id: Default::default(),
            user_id: Default::default(),
            target: Default::default(),
            sender: Default::default(),
            tags: Default::default(),
            data,
        }
    }

    pub fn finish_sending(&mut self) {
        self.recv.close();
    }

    pub async fn read(&mut self) -> Response {
        let data = self
            .recv
            .recv()
            .await
            .expect("expected to read a response")
            .to_string();
        let msg = twitch_message::parse(&data).unwrap().message;

        Response {
            reply_parent_msg_id: msg.tags.get("reply-parent-msg-id").map(ToString::to_string),
            channel: msg
                .args
                .get(0)
                .map(ToString::to_string)
                .expect("channel attached to message"),
            data: msg
                .data
                .map(|s| s.to_string())
                .expect("data attached to message"),
        }
    }
}

pub struct SendGuard<'a, 'e> {
    inner: &'a MockBinding,
    id: Option<&'e str>,
    user_id: Option<&'e str>,
    target: Option<&'e str>,
    sender: Option<&'e str>,
    tags: TagsBuilder,
    data: &'e str,
}

impl<'a, 'e> SendGuard<'a, 'e> {
    pub async fn send(mut self) {
        if !self.tags.has("id") {
            self.tags = self.tags.add("id", self.id.unwrap_or("test_msg_id"))
        }

        if !self.tags.has("user-id") {
            self.tags = self.tags.add("user-id", self.user_id.unwrap_or("12345"))
        }

        let pm = Privmsg::builder()
            .channel(self.target.unwrap_or("#test"))
            .sender(self.sender.unwrap_or("test_user"))
            .data(self.data)
            .tags(self.tags.finish())
            .finish_privmsg()
            .unwrap();

        self.inner.send_privmsg(pm).await;
    }

    pub fn with_id(self, id: &'e str) -> Self {
        Self {
            id: Some(id),
            ..self
        }
    }

    pub fn with_user_id(self, user_id: &'e str) -> Self {
        Self {
            user_id: Some(user_id),
            ..self
        }
    }

    pub fn with_target(self, target: &'e str) -> Self {
        Self {
            target: Some(target),
            ..self
        }
    }

    pub fn with_sender(self, sender: &'e str) -> Self {
        Self {
            sender: Some(sender),
            ..self
        }
    }

    pub fn add_tag(self, key: &str, val: &str) -> Self {
        Self {
            tags: self.tags.add(key, val),
            ..self
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub reply_parent_msg_id: Option<String>,
    pub channel: String,
    pub data: String,
}

impl Response {
    pub fn new<'a>(
        reply_parent_msg_id: impl Into<Option<&'a str>>,
        channel: &str,
        data: &str,
    ) -> Self {
        Self {
            reply_parent_msg_id: reply_parent_msg_id.into().map(ToString::to_string),
            channel: channel.to_string(),
            data: data.to_string(),
        }
    }

    pub fn builder(data: &str) -> ResponseBuilder {
        ResponseBuilder {
            data: data.to_string(),
            ..Default::default()
        }
    }
}

#[derive(Default)]
pub struct ResponseBuilder {
    reply_parent_msg_id: Option<String>,
    channel: Option<String>,
    data: String,
}

impl ResponseBuilder {
    pub fn reply_parent_msg_id(mut self, reply_parent_msg_id: impl ToString) -> Self {
        self.reply_parent_msg_id
            .replace(reply_parent_msg_id.to_string());
        self
    }

    pub fn channel(mut self, channel: impl ToString) -> Self {
        self.channel.replace(channel.to_string());
        self
    }

    pub fn data(mut self, data: impl ToString) -> Self {
        self.data = data.to_string();
        self
    }

    pub fn build(self) -> Response {
        Response {
            reply_parent_msg_id: self.reply_parent_msg_id,
            channel: self.channel.unwrap_or_else(|| String::from("#test")),
            data: self.data,
        }
    }
}
