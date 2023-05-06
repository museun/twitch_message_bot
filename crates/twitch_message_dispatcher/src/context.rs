use std::{borrow::Borrow, ops::Deref, sync::Arc};

use twitch_message::messages::Privmsg;
use twitch_message_bot::Writer;

use crate::Arguments;

pub struct Context {
    pub msg: Arc<Privmsg<'static>>,
    pub writer: Writer,
    pub arguments: Arguments,
}

impl Context {
    pub fn sender(&self) -> &str {
        self.msg.sender.deref().borrow()
    }

    pub fn channel(&self) -> &str {
        &self.msg.channel
    }

    pub fn user_id(&self) -> &str {
        self.msg.user_id().expect("user-id attached").borrow()
    }

    pub fn message_id(&self) -> &str {
        self.msg.msg_id().expect("msg-id attached").borrow()
    }

    pub fn reply(&self, data: impl ToString) {
        self.writer.reply(&self.msg, data)
    }

    pub fn say(&self, data: impl ToString) {
        self.writer.privmsg(&self.msg, data)
    }
}

impl std::ops::Index<&str> for Context {
    type Output = str;

    fn index(&self, index: &str) -> &Self::Output {
        &self.arguments[index]
    }
}
