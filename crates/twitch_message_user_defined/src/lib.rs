use std::{collections::HashMap, sync::Arc};

use twitch_message_bot::{twitch_message::messages::Privmsg, Writer};
use twitch_message_dispatcher::{Access, Command, Context, PrivmsgAccess};

#[derive(Debug)]
#[non_exhaustive]
pub enum UserDefinedError {
    CommandExists {
        command: String,
    },
    CommandDoesNotExist {
        command: String,
    },
    LoadError {
        error: Box<dyn std::error::Error + Send + Sync>,
    },
    SaveError {
        error: Box<dyn std::error::Error + Send + Sync>,
    },
}

impl std::fmt::Display for UserDefinedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CommandExists { command } => write!(f, "command already exists: {command}"),
            Self::CommandDoesNotExist { command } => {
                write!(f, "command does not exist: {command}")
            }
            Self::LoadError { error } => write!(f, "cannot load: {error}"),
            Self::SaveError { error } => write!(f, "cannot save: {error}"),
        }
    }
}

impl std::error::Error for UserDefinedError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::LoadError { error } => Some(&**error),
            Self::SaveError { error } => Some(&**error),
            _ => None,
        }
    }
}

#[derive(Default)]
#[cfg_attr(feature = "serde", derive(::serde::Serialize, ::serde::Deserialize))]
pub struct UserDefined {
    #[cfg_attr(feature = "serde", serde(flatten))]
    commands: HashMap<String, UserCommand>,
}

impl UserDefined {
    pub fn load_from_str<E>(
        input: &str,
        load: impl Fn(&str) -> Result<Self, E>,
        update_help: impl Fn(&Command),
    ) -> Result<Self, UserDefinedError>
    where
        Self: 'static,
        E: std::error::Error + Send + Sync + 'static,
    {
        let this = load(input).map_err(|error| UserDefinedError::LoadError {
            error: Box::new(error),
        })?;

        for (k, v) in &this.commands {
            update_help(&Self::fake_command(&k, &v.body, v.allowed.clone()));
        }

        Ok(this)
    }

    pub fn add<E>(
        &mut self,
        command: &str,
        body: &str,
        save: impl Fn(&Self) -> Result<(), E>,
        update_help: impl Fn(&Command),
    ) -> Result<(), UserDefinedError>
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        use std::collections::hash_map::Entry::*;

        let Vacant(entry) = self.commands.entry(command.to_string()) else {
            return Err(UserDefinedError::CommandExists {
                command: command.to_string(),
            })
        };

        let val = entry.insert(UserCommand {
            body: body.to_string(),
            allowed: vec![],
        });

        update_help(&Self::fake_command(
            &command,
            &val.body,
            val.allowed.clone(),
        ));

        save(&self).map_err(|error| UserDefinedError::SaveError {
            error: Box::new(error),
        })
    }

    pub fn update<E>(
        &mut self,
        command: &str,
        update: impl Fn(&mut UserCommand),
        save: impl Fn(&Self) -> Result<(), E>,
        update_help: impl Fn(&Command),
    ) -> Result<(), UserDefinedError>
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        let cmd = self
            .commands
            .get_mut(command)
            .map(|cmd| {
                update(cmd);
                cmd
            })
            .ok_or_else(|| UserDefinedError::CommandDoesNotExist {
                command: command.to_string(),
            })?;

        update_help(&Self::fake_command(
            &command,
            &cmd.body,
            cmd.allowed.clone(),
        ));

        save(&self).map_err(|error| UserDefinedError::SaveError {
            error: Box::new(error),
        })
    }

    pub fn remove<E>(
        &mut self,
        command: &str,
        save: impl Fn(&Self) -> Result<(), E>,
        remove_help: impl Fn(&Command),
    ) -> Result<UserCommand, UserDefinedError>
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        let cmd =
            self.commands
                .remove(command)
                .ok_or_else(|| UserDefinedError::CommandDoesNotExist {
                    command: command.to_string(),
                })?;

        remove_help(&Self::fake_command(
            &command,
            &cmd.body,
            cmd.allowed.clone(),
        ));

        save(&self)
            .map_err(|error| UserDefinedError::SaveError {
                error: Box::new(error),
            })
            .map(|_| cmd)
    }

    pub fn dispatch(&self, msg: &Privmsg<'_>, writer: &Writer) -> bool {
        if let Some(udc) = self.commands.get(&*msg.data) {
            if msg.is_allowed(&udc.allowed) {
                writer.privmsg(msg, &udc.body);
                return true;
            }
        }
        false
    }

    pub fn get_all_names(&self) -> impl Iterator<Item = &str> + ExactSizeIterator {
        self.commands.keys().map(AsRef::as_ref)
    }

    pub fn find(&self, cmd: &str) -> Option<&UserCommand> {
        self.commands.get(cmd)
    }

    pub fn find_mut(&mut self, cmd: &str) -> Option<&mut UserCommand> {
        self.commands.get_mut(cmd)
    }
}

impl UserDefined {
    // !set <!command> <body>
    pub async fn add_command(this: Arc<tokio::sync::Mutex<Self>>, ctx: Context) {
        if let Some(cmd) = ctx.arguments.take("command") {
            if let Some(body) = ctx.arguments.take("body") {
                let mut this = this.lock().await;

                *this.commands.entry(cmd).or_default() = UserCommand {
                    body,
                    allowed: vec![],
                };
            }
        }
        todo!();
    }

    // !modify <!command> <access..>
    pub async fn modify_command(this: Arc<tokio::sync::Mutex<Self>>, ctx: Context) {
        if let Some(cmd) = ctx.arguments.get("command") {
            ctx.arguments.get(key)
        }
    }

    pub async fn remove_command(this: Arc<tokio::sync::Mutex<Self>>, ctx: Context) {
        todo!();
    }

    pub async fn dispatch(
        this: Arc<tokio::sync::Mutex<Self>>,
        msg: &Privmsg<'static>,
        writer: &Writer,
    ) {
        let this = this.lock().await;
        if let Some(udc) = this.commands.get(&*msg.data) {
            if msg.is_allowed(&udc.allowed) {
                writer.privmsg(msg, &udc.body);
            }
        }
    }
}

impl UserDefined {
    fn fake_command_id(cmd: &str) -> String {
        format!("__user_defined_{cmd}")
    }

    fn fake_command(cmd: &str, body: &str, allowed: impl IntoIterator<Item = Access>) -> Command {
        Command {
            id: Self::fake_command_id(cmd),
            command: String::from(cmd),
            description: String::from(body),
            allowed: Vec::from_iter(allowed),
            arguments: <_>::default(),
            aliases: Vec::new(),
        }
    }
}

#[cfg_attr(feature = "serde", derive(::serde::Serialize, ::serde::Deserialize))]
#[derive(Default, Debug, Clone)]
pub struct UserCommand {
    pub body: String,
    #[cfg_attr(feature = "serde", serde(default))]
    pub allowed: Vec<Access>,
}
