use serde::Deserialize;
use std::{borrow::Borrow, collections::HashMap};

use crate::Command;

#[non_exhaustive]
#[derive(Debug)]
pub enum CommandFileError {
    NotInitialized,
    IdNotFound {
        id: String,
    },
    CannotReadFile {
        error: std::io::Error,
    },
    CannotDeserialize {
        error: Box<dyn std::error::Error + Send + Sync>,
    },
}

impl std::fmt::Display for CommandFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotInitialized => write!(f, "CommandFile is not initialized"),
            Self::IdNotFound { id } => write!(f, "Command id '{id}' not found"),
            Self::CannotReadFile { error } => write!(f, "Cannot read file: {error}"),
            Self::CannotDeserialize { error } => write!(f, "Cannot deserialize file: {error}"),
        }
    }
}

impl std::error::Error for CommandFileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CannotReadFile { error } => Some(error),
            Self::CannotDeserialize { error } => Some(&**error),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(transparent)]
pub(crate) struct Id(String);

impl PartialEq<str> for Id {
    fn eq(&self, other: &str) -> bool {
        self.0.as_str().eq(other)
    }
}

impl Borrow<str> for Id {
    fn borrow(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct CommandFile {
    commands: HashMap<Id, Command>,
}

impl<'de> Deserialize<'de> for CommandFile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut commands = <HashMap<Id, Command>>::deserialize(deserializer)?;

        for (Id(id), cmd) in commands.iter_mut() {
            cmd.id = id.to_string();
        }

        Ok(Self { commands })
    }
}

impl CommandFile {
    pub fn load_from_str<E>(
        data: &str,
        deser: impl FnOnce(&str) -> Result<Self, E>,
    ) -> Result<(), CommandFileError>
    where
        Self: 'static,
        E: std::error::Error + Send + Sync + 'static,
    {
        let mut this = deser(data).map_err(|error| CommandFileError::CannotDeserialize {
            error: Box::new(error),
        })?;

        if let Some(old) = COMMAND_FILE.get() {
            let old = std::mem::take(&mut old.write().commands);
            let mut help = crate::help::help_registry();

            for id in old.into_keys() {
                if !this.commands.contains_key(&id) {
                    help.remove(&id.0);
                }
            }
        }

        for (Id(id), cmd) in this.commands.iter_mut() {
            cmd.id = id.clone();
            crate::help::help_registry().register(cmd);
        }

        if let Err(this) = COMMAND_FILE.set(parking_lot::RwLock::new(this)) {
            let file = COMMAND_FILE
                .get()
                .expect("valid command_file initialization");
            *file.write() = this.into_inner();
        }

        Ok(())
    }

    pub fn lookup(id: &str) -> Result<Command, CommandFileError> {
        COMMAND_FILE
            .get()
            .ok_or_else(|| CommandFileError::NotInitialized)?
            .read()
            .commands
            .get(id)
            .cloned()
            .ok_or_else(|| CommandFileError::IdNotFound { id: id.to_string() })
    }

    pub(crate) fn get_ref(
        id: &str,
    ) -> Result<parking_lot::MappedRwLockReadGuard<'_, Command>, CommandFileError> {
        let g = COMMAND_FILE
            .get()
            .ok_or_else(|| CommandFileError::NotInitialized)?
            .read();

        parking_lot::RwLockReadGuard::try_map(g, |item| item.commands.get(id))
            .map_err(|_| CommandFileError::IdNotFound { id: id.to_string() })
    }

    pub(crate) fn add(cmd: &Command) -> Result<Option<Command>, CommandFileError> {
        Ok(COMMAND_FILE
            .get()
            .ok_or_else(|| CommandFileError::NotInitialized)?
            .write()
            .commands
            .insert(Id(cmd.id.clone()), cmd.clone()))
    }
}

static COMMAND_FILE: once_cell::sync::OnceCell<parking_lot::RwLock<CommandFile>> =
    once_cell::sync::OnceCell::new();
