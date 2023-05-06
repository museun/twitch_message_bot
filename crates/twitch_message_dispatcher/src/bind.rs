use std::{future::Future, pin::Pin, sync::Arc};

use twitch_message::messages::Privmsg;
use twitch_message_bot::Writer;

use crate::{command::Access, Arguments, Command, Context, Match, Outcome};

pub(crate) type BoxFuture<'a, T = ()> = Pin<Box<dyn Future<Output = T> + Send + Sync + 'a>>;
pub(crate) type Callable =
    dyn Fn(Arc<Privmsg<'static>>, Writer) -> BoxFuture<'static> + Send + Sync + 'static;

#[derive(Debug, Copy, Clone)]
#[non_exhaustive]
pub struct BindOptions {
    pub report_invalid_usage: bool,
    pub report_command_error: bool,
    pub report_access_error: bool,
    pub use_command_file: bool,
}

impl Default for BindOptions {
    fn default() -> Self {
        Self {
            report_invalid_usage: true,
            report_command_error: true,
            report_access_error: false,
            use_command_file: false,
        }
    }
}

pub struct Bind<T>
where
    T: Send + Sync + 'static,
{
    this: Arc<tokio::sync::Mutex<T>>,
    handlers: Vec<Arc<Callable>>,
}

impl<T> Bind<T>
where
    T: Send + Sync + 'static,
{
    pub fn create(this: T) -> Self {
        Self {
            this: Arc::new(tokio::sync::Mutex::new(this)),
            handlers: Vec::new(),
        }
    }

    pub fn bind<F, Fut, O>(mut self, cmd: Command, handler: F, opts: BindOptions) -> Self
    where
        F: Fn(Arc<tokio::sync::Mutex<T>>, Context) -> Fut + Send + Sync + 'static + Copy,
        Fut: Future<Output = O> + Send + Sync + 'static,
        O: Outcome,
    {
        let this = Arc::clone(&self.this);
        let cmd = Arc::new(cmd);

        let opts = opts;

        crate::help::help_registry().register(&cmd);

        if opts.use_command_file {
            // TODO: the commandfile may not be initialized, should we return an error?
            crate::CommandFile::add(&cmd).expect("command file is initialized");
        }

        let this = move |msg: Arc<Privmsg<'static>>, writer: Writer| -> BoxFuture<'static> {
            let this = Arc::clone(&this);
            let cmd = Arc::clone(&cmd);

            let fut = async move {
                let arguments = {
                    let Some(args) = (if opts.use_command_file {
                        match crate::CommandFile::get_ref(&cmd.id) {
                            Ok(cmd) => Self::check_cmd_access(&cmd, &msg, &writer, opts),
                            _ => Self::check_cmd_access(&cmd, &msg, &writer, opts),
                        }
                    } else {
                        Self::check_cmd_access(&cmd, &msg, &writer, opts)
                    }) else { return };

                    args
                };

                let outcome = {
                    let context = Context {
                        msg: Arc::clone(&msg),
                        writer: writer.clone(),
                        arguments,
                    };
                    handler(this, context).await
                };

                if opts.report_command_error {
                    if let Some(error) = outcome.as_error() {
                        writer.reply(&msg, error);
                    }
                }
            };

            Box::pin(fut)
        };

        self.handlers.push(Arc::new(this) as _);
        self
    }

    // XXX: why isn't this an error?
    pub fn listen<F, Fut, O>(mut self, handler: F, opts: BindOptions) -> Self
    where
        F: Fn(&mut T, &Privmsg<'static>, &Writer) -> Fut + Send + Sync + 'static + Copy,
        Fut: Future<Output = O> + Send + Sync + 'static,
        O: Outcome,
    {
        let this = Arc::clone(&self.this);
        let opts = opts;

        let this = move |msg: Arc<Privmsg<'static>>, writer: Writer| -> BoxFuture<'static> {
            let this = Arc::clone(&this);

            Box::pin(async move {
                let mut guard = this.lock().await;
                let this = &mut *guard;
                if let Some(err) = handler(this, &msg, &writer).await.as_error() {
                    if opts.report_command_error {
                        writer.reply(&msg, err);
                    }
                }
            })
        };

        self.handlers.push(Arc::new(this) as _);
        self
    }

    pub fn finish(self) -> Arc<Callable> {
        let this = Arc::new(self);

        Arc::new(
            move |msg: Arc<Privmsg<'static>>, writer: Writer| -> BoxFuture<'static> {
                let this = Arc::clone(&this);
                let fut = async move {
                    let mut set = tokio::task::JoinSet::default();
                    for handler in this.handlers.iter().map(Arc::clone) {
                        let msg = Arc::clone(&msg);
                        let writer = writer.clone();
                        set.spawn((handler)(msg, writer));
                    }

                    while let Some(..) = set.join_next().await {}
                };
                Box::pin(fut)
            },
        )
    }

    fn check_cmd_access(
        cmd: &Command,
        msg: &Privmsg<'_>,
        writer: &Writer,
        opts: BindOptions,
    ) -> Option<Arguments> {
        let allowed = cmd.is_allowed(msg);

        match Self::extract_args(cmd, msg) {
            Ok(Some(map)) if allowed => return Some(map),
            Err(err) if allowed && opts.report_invalid_usage => {
                writer.reply(msg, err);
                return None;
            }
            Ok(None) => return None,
            _ => {}
        }

        if opts.report_access_error && !allowed {
            writer.reply(msg, "you cannot use that command");
        }

        None
    }

    pub(crate) fn extract_args(
        cmd: &Command,
        msg: &Privmsg<'_>,
    ) -> Result<Option<Arguments>, String> {
        if cmd.arguments.args.is_empty() && cmd.is_command_match(&msg.data) {
            return Ok(Some(Arguments::default()));
        }

        let Some(tail) = cmd.tail(&msg.data) else {
            return Ok(None)
        };

        match cmd.arguments.extract(tail) {
            Match::Required => Err(format!("usage: {} {}", cmd.command, cmd.arguments.usage)),
            Match::NoMatch => Ok(None),
            Match::Match(map) => Ok(Some(Arguments { map })),
        }
    }
}
