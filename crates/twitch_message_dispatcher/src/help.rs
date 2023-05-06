use std::collections::BTreeMap;

use crate::{command::Access, Command};

#[derive(Debug)]
pub struct Help {
    pub command: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub usage: String,
    pub access: Vec<Access>,
}

#[derive(Default)]
pub struct HelpRegistry {
    // TODO this is sorted, we only use this for duplicate detection
    help: BTreeMap<String, Help>,
}

impl HelpRegistry {
    pub(crate) fn register(&mut self, cmd: &Command) {
        let help = Help {
            command: cmd.command.clone(),
            aliases: cmd.aliases.to_vec(),
            description: cmd.description.clone(),
            usage: cmd.arguments.usage.to_string(),
            access: cmd.allowed.clone(),
        };

        self.help.insert(cmd.id.clone(), help);
    }

    pub fn remove(&mut self, id: &str) {
        self.help.remove(id);
    }

    // TODO swap 'command' and 'alias' here
    pub fn lookup<'a>(&'a self, cmd: &str) -> Option<&'a Help> {
        self.help.iter().find_map(|(_, v)| {
            (v.command == cmd || v.aliases.iter().any(|c| c == cmd)).then_some(v)
        })
    }

    pub fn get_all(&self) -> impl Iterator<Item = (&str, &Help)> + ExactSizeIterator {
        self.help.values().map(|v| (&*v.command, v))
    }
}

static HELP_REGISTRY: once_cell::sync::Lazy<parking_lot::Mutex<HelpRegistry>> =
    once_cell::sync::Lazy::new(Default::default);

pub(crate) fn help_registry() -> parking_lot::MutexGuard<'static, HelpRegistry> {
    HELP_REGISTRY.lock()
}
