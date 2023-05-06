use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use twitch_message::messages::Privmsg;

use crate::ExampleArgs;

#[derive(Debug, Clone, PartialEq, PartialOrd, Deserialize)]
pub struct Command {
    #[serde(skip)]
    pub id: String,
    pub command: String,
    pub description: String,

    #[serde(default)]
    pub allowed: Vec<Access>,

    #[serde(default)]
    pub arguments: ExampleArgs,
    #[serde(default)]
    pub aliases: Vec<String>,
}

impl serde::Serialize for Command {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct as _;
        let mut s = serializer.serialize_struct("Command", 5)?;
        s.serialize_field("command", &self.command)?;
        s.serialize_field("arguments", &self.arguments.usage)?;
        s.serialize_field("aliases", &self.aliases)?;
        s.serialize_field("description", &self.description)?;
        s.serialize_field("allowed", &self.allowed)?;
        s.end()
    }
}

impl Command {
    pub fn builder(
        id: impl ToString,
        command: impl ToString,
        description: impl ToString,
    ) -> CommandBuilder {
        let (id, command, description) =
            (id.to_string(), command.to_string(), description.to_string());

        CommandBuilder {
            id: id.trim().to_string(),
            command: command.trim().to_string(),
            description: description.trim().to_string(),
            args: ExampleArgs::default(),
            aliases: Vec::new(),
            seen: HashSet::new(),
            allowed: Vec::new(),
        }
    }

    pub fn is_allowed(&self, pm: &Privmsg<'_>) -> bool {
        pm.is_allowed(&self.allowed)
    }

    pub(crate) fn tail<'a>(&self, data: &'a str) -> Option<&'a str> {
        for cmd in self.possible_commands() {
            if cmd.len() > data.len() {
                continue;
            }

            if &data[..cmd.len()] == cmd {
                return data.get(cmd.len()..).map(<str>::trim);
            }
        }
        None
    }

    // TODO this iterates twice
    pub(crate) fn is_command_match(&self, query: &str) -> bool {
        self.possible_commands().any(|c| c == query)
    }

    fn possible_commands(&self) -> impl Iterator<Item = &String> {
        std::iter::once(&self.command).chain(self.aliases.iter())
    }
}

pub struct CommandBuilder {
    id: String,
    command: String,
    description: String,
    args: ExampleArgs,
    aliases: Vec<String>,
    seen: HashSet<String>,
    allowed: Vec<Access>,
}

impl CommandBuilder {
    pub fn args(self, args: ExampleArgs) -> Self {
        Self { args, ..self }
    }

    pub fn alias(mut self, alias: impl ToString) -> Self {
        if self.seen.insert(alias.to_string()) {
            self.aliases.push(alias.to_string())
        }
        self
    }

    pub fn allow(mut self, access: Access) -> Self {
        self.allowed.push(access);
        self
    }

    pub fn build(self) -> Result<Command, CommandBuilderError> {
        Ok(Command {
            id: (!self.id.is_empty())
                .then_some(self.id)
                .ok_or(CommandBuilderError::MissingId)?,

            command: (!self.command.is_empty())
                .then_some(self.command)
                .ok_or(CommandBuilderError::MissingCommand)?,

            description: (!self.description.is_empty())
                .then_some(self.description)
                .ok_or(CommandBuilderError::MissingDescription)?,

            allowed: self.allowed,

            aliases: self.aliases,
            arguments: self.args,
        })
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub enum CommandBuilderError {
    MissingId,
    MissingCommand,
    MissingDescription,
}

impl std::fmt::Display for CommandBuilderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingId => f.write_str("Missing id"),
            Self::MissingCommand => f.write_str("Missing command"),
            Self::MissingDescription => f.write_str("Missing description"),
        }
    }
}

impl std::error::Error for CommandBuilderError {}

#[derive(Clone, Debug, Default, PartialEq, PartialOrd, Eq, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Access {
    Moderator,
    Broadcaster,
    Subscriber,
    Vip,
    User {
        name: String,
    },
    UserId {
        id: String,
    },
    #[default]
    All,
}

pub trait PrivmsgAccess {
    fn is_allowed(&self, access: &[Access]) -> bool;
}

impl PrivmsgAccess for Privmsg<'_> {
    fn is_allowed(&self, access: &[Access]) -> bool {
        if access.is_empty() {
            return true;
        }

        let user_name = self.sender.as_str();
        let Some(user_id) = self.user_id().map(|c| c.as_str()) else { return true };

        for access in access {
            return match access {
                Access::Moderator if self.is_from_moderator() => true,
                Access::Broadcaster if self.is_from_broadcaster() => true,
                Access::Subscriber if self.is_from_subscriber() => true,
                Access::Vip if self.is_from_vip() => true,
                Access::User { name } if name.eq_ignore_ascii_case(user_name) => true,
                Access::UserId { id } if id == user_id => true,
                Access::All => true,
                _ => continue,
            };
        }

        false
    }
}
