use std::{collections::HashMap, sync::Arc};

use twitch_message::messages::Privmsg;

use twitch_message_bot::Writer;

use crate::{bind::Callable, help::Help, Bind, Command, Match, PrivmsgAccess};

#[derive(Default)]
pub struct DispatcherBuilder {
    callables: Vec<Arc<Callable>>,
    help_cmd: Option<Command>,
}

impl DispatcherBuilder {
    pub fn add_bind<T>(mut self, bind: Bind<T>) -> Self
    where
        T: Send + Sync + 'static,
    {
        self.callables.push(bind.finish() as _);
        self
    }

    pub fn with_help(self, help_command: &str, help_description: &str) -> Self {
        let help_cmd = Command::builder(
            concat!(
                "__",
                env!("CARGO_PKG_NAME"),
                "@",
                env!("CARGO_PKG_VERSION"),
                "_help_command"
            ),
            help_command,
            help_description,
        )
        .args("<command?>".parse().unwrap())
        .build()
        .unwrap();

        crate::help::help_registry().register(&help_cmd);
        Self {
            help_cmd: Some(help_cmd),
            ..self
        }
    }

    pub fn into_dispatcher(self) -> Dispatcher {
        Dispatcher {
            callables: Arc::from(self.callables.into_boxed_slice()),
            help_cmd: self.help_cmd.map(Arc::new),
        }
    }
}

#[derive(Clone)]
pub struct Dispatcher {
    callables: Arc<[Arc<Callable>]>,
    help_cmd: Option<Arc<Command>>,
}

impl Dispatcher {
    pub fn builder() -> DispatcherBuilder {
        DispatcherBuilder::default()
    }

    pub async fn dispatch_async(&self, msg: Arc<Privmsg<'static>>, writer: Writer) {
        if let Some(help) = &self.help_cmd {
            if let Some(tail) = help.tail(&msg.data) {
                if let Match::Match(args) = help.arguments.extract(tail) {
                    Self::try_send_help(&args, &msg, &writer);
                    return;
                }
            }
        }

        let mut set = tokio::task::JoinSet::default();
        for callable in self.callables.iter().map(Arc::clone) {
            set.spawn((callable)(Arc::clone(&msg), writer.clone()));
        }

        while let Some(..) = set.join_next().await {}
    }

    pub fn dispatch(&self, msg: Arc<Privmsg<'static>>, writer: Writer) {
        let this = self.clone();
        tokio::spawn(async move { this.dispatch_async(msg, writer).await });
    }

    pub fn help_register(cmd: &Command) {
        crate::help::help_registry().register(cmd);
    }

    pub fn help_remove(cmd: &Command) {
        crate::help::help_registry().remove(&cmd.id);
    }

    // TODO use the `Access` type to show the user what they can use
    fn try_send_help(args: &HashMap<String, String>, msg: &Privmsg, writer: &Writer) {
        use std::borrow::Cow;

        let help = crate::help::help_registry();
        match args.get("command") {
            Some(cmd) => {
                let Some(help) = help.lookup(cmd) else {
                    return writer.reply(msg, format!("unknown command: {cmd}"))
                };

                if !msg.is_allowed(&help.access) {
                    return writer.reply(msg, format!("unknown command: {cmd}"));
                }

                let mut reply = help.command.to_string();
                if !help.usage.is_empty() {
                    reply.push(' ');
                    reply.push_str(&help.usage);
                }

                reply.push_str(": ");
                reply.push_str(&help.description);

                match help.aliases.len() {
                    0 => {}
                    1 => reply.push_str(&format!("\navailable alias: {}", &help.aliases[0])),
                    _ => {
                        reply.push_str("\navailable aliases: ");
                        for (i, alias) in help.aliases.iter().enumerate() {
                            if i > 0 {
                                reply.push_str(", ");
                            }
                            reply.push_str(alias);
                        }
                    }
                }

                writer.reply(msg, reply)
            }

            None => writer.reply(
                msg,
                help.get_all()
                    .filter(|(_, Help { access, .. })| msg.is_allowed(access))
                    .map(|(c, help)| match help.aliases.len() {
                        0 => Cow::from(c),
                        1 => Cow::from(format!("{c} (alias: {})", help.aliases[0])),
                        _ => Cow::from(format!(
                            "{c} (aliases: {})",
                            help.aliases.iter().fold(String::new(), |mut a, c| {
                                if !a.is_empty() {
                                    a.push_str(", ");
                                }
                                a.push_str(c);
                                a
                            })
                        )),
                    })
                    .join_multiline_max(10),
            ),
        }
    }
}

trait IterExt
where
    Self: Sized + Iterator,
    Self::Item: AsRef<str>,
{
    fn join_with(self, sp: &str) -> String {
        self.fold(String::new(), |mut a, c| {
            if !a.is_empty() {
                a.push_str(sp);
            }
            a.push_str(c.as_ref());
            a
        })
    }

    fn join_multiline_max(self, max: usize) -> String {
        self.enumerate().fold(String::new(), |mut a, (i, c)| {
            if i > 0 && i % max == 0 {
                a.push('\n')
            }

            if !a.is_empty() {
                a.push(' ');
            }
            a.push_str(c.as_ref());
            a
        })
    }
}

impl<I> IterExt for I
where
    I: Iterator,
    I::Item: AsRef<str>,
{
}
