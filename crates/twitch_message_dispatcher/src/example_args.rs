use std::collections::{HashMap, HashSet};

#[derive(Default, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExampleArgs {
    pub usage: Box<str>,
    pub args: Box<[ArgType]>,
}

impl<'de> serde::Deserialize<'de> for ExampleArgs {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error as _;
        let s = <std::borrow::Cow<'_, str>>::deserialize(deserializer)?;
        let s = s.trim();

        (!s.is_empty())
            .then(|| s.parse().map_err(D::Error::custom))
            .unwrap_or_else(|| Ok(Self::default()))
    }
}

impl ExampleArgs {
    pub fn extract(&self, mut input: &str) -> Match {
        use ArgKind::*;

        if input.is_empty() {
            if self.contains(&Required) {
                return Match::Required;
            }

            if !self.args.is_empty() && (!self.contains(&Optional) && !self.contains(&Variadic)) {
                return Match::NoMatch;
            }
        }

        if !input.is_empty() && self.args.is_empty() {
            return Match::NoMatch;
        }

        let mut map = HashMap::default();

        for ArgType { key, kind } in &*self.args {
            match (kind, input.find(' ')) {
                (Required | Optional, None) | (Variadic, ..) => {
                    if !input.is_empty() {
                        map.insert(key.to_string(), input.into());
                    }
                    break;
                }

                (.., Some(pos)) => {
                    let (head, tail) = input.split_at(pos);
                    map.insert(key.to_string(), head.into());
                    input = tail.trim();
                }
            }
        }

        Match::Match(map)
    }

    fn contains(&self, arg: &ArgKind) -> bool {
        self.args.iter().any(|ArgType { kind, .. }| kind == arg)
    }

    fn validate(args: &[ArgType]) -> Result<(), ExampleError> {
        let duplicates = args.iter().fold(vec![], |mut a, ArgType { kind, key }| {
            if matches!(kind, ArgKind::Variadic) {
                a.push(key.to_string());
            }
            a
        });

        if duplicates.len() > 1 {
            return Err(ExampleError::MultipleVariadic { keys: duplicates });
        }

        let mut iter = args.iter().peekable();
        while let Some(ArgType { key, kind }) = iter.next() {
            if matches!(kind, ArgKind::Optional)
                && matches!(iter.peek(), Some(ArgType{kind, ..}) if matches!(kind, ArgKind::Required))
            {
                return Err(ExampleError::OptionalBeforeRequired {
                    key: key.to_string(),
                });
            }

            if matches!(kind, ArgKind::Variadic) && iter.peek().is_some() {
                return Err(ExampleError::VariadicNotInTail {
                    key: key.to_string(),
                });
            }
        }

        Ok(())
    }
}

impl std::str::FromStr for ExampleArgs {
    type Err = ExampleError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let input = input.trim();
        if input.is_empty() {
            return Err(Self::Err::EmptyInput);
        }

        let mut seen = HashSet::new();
        let mut args = vec![];

        let all_alpha = move |s: &[u8], ctor: ArgKind| {
            if s.iter()
                .all(|d| matches!(d, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' |  b'_' | b'-' ))
            {
                Ok(ctor)
            } else {
                Err(Self::Err::InvalidKey {
                    key: String::from_utf8_lossy(s).to_string(),
                })
            }
        };

        for token in input.split_whitespace() {
            let mut append = |arg: &[_]| {
                let data = &token[1..=arg.len()];
                if !seen.insert(data) {
                    return Err(Self::Err::Duplicate {
                        key: data.to_string(),
                    });
                }
                Ok(data.into())
            };

            let arg = match token.as_bytes() {
                [b'<', arg @ .., b'.', b'.', b'>'] => ArgType {
                    key: append(arg)?,
                    kind: all_alpha(arg, ArgKind::Variadic)?,
                },
                [b'<', arg @ .., b'?', b'>'] => ArgType {
                    key: append(arg)?,
                    kind: all_alpha(arg, ArgKind::Optional)?,
                },
                [b'<', arg @ .., b'>'] => ArgType {
                    key: append(arg)?,
                    kind: all_alpha(arg, ArgKind::Required)?,
                },
                _ => continue,
            };

            args.push(arg);
        }

        Self::validate(&args).map(|_| Self {
            usage: input.into(),
            args: args.into(),
        })
    }
}

#[non_exhaustive]
#[derive(Debug, PartialEq, Eq)]
pub enum ExampleError {
    Duplicate { key: String },
    MultipleVariadic { keys: Vec<String> },
    VariadicNotInTail { key: String },
    InvalidKey { key: String },
    OptionalBeforeRequired { key: String },
    EmptyInput,
    InvalidCommand { input: String },
}

impl std::fmt::Display for ExampleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Duplicate { key } => write!(f, "duplicate key: {key}"),
            Self::MultipleVariadic { keys } => write!(
                f,
                "multiple variadics: {}",
                keys.iter().fold(String::new(), |mut a, c| {
                    if !a.is_empty() {
                        a.push_str(", ");
                    }
                    a.push_str(c);
                    a
                })
            ),
            Self::VariadicNotInTail { key } => {
                write!(f, "variadic '{key}' not in tail position")
            }
            Self::InvalidKey { key } => {
                write!(
                    f,
                    "invalid key: '{key}'. only A-Za-z0-9 and - and _ are allowed"
                )
            }
            Self::OptionalBeforeRequired { key } => {
                write!(f, "optional used before a required key: {key}")
            }
            Self::EmptyInput => f.write_str("argument input was empty"),
            Self::InvalidCommand { input } => write!(f, "cannot parse '{input}' as a command"),
        }
    }
}

impl std::error::Error for ExampleError {}

#[derive(Debug, Clone, Default)]
pub struct Arguments {
    pub map: HashMap<String, String>,
}

impl Arguments {
    pub fn take(&mut self, key: &str) -> String {
        self.map
            .remove(key)
            .unwrap_or_else(|| panic!("{key} must exist"))
    }

    pub fn take_parsed<T>(&mut self, key: &str) -> Result<T, T::Err>
    where
        T: std::str::FromStr,
    {
        self.take(key).parse()
    }

    pub fn take_many(&mut self, key: &str) -> Vec<String> {
        self.take_many_by(key, " ")
    }

    pub fn take_many_by(&mut self, key: &str, sep: &str) -> Vec<String> {
        self.map
            .remove(key)
            .map(|s| s.split(sep).map(|s| s.to_string()).collect())
            .unwrap_or_default()
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(|s| &**s)
    }

    pub fn get_many(&self, key: &str) -> Vec<&str> {
        self.get_many_by(key, " ")
    }

    pub fn get_many_by(&self, key: &str, sep: &str) -> Vec<&str> {
        self.map
            .get(key)
            .into_iter()
            .flat_map(|s| s.split(sep))
            .collect()
    }

    pub fn get_parsed<T>(&self, key: &str) -> Option<Result<T, T::Err>>
    where
        T: std::str::FromStr,
    {
        self.get(key).map(<str>::parse)
    }
}

impl std::ops::Index<&str> for Arguments {
    type Output = str;

    fn index(&self, index: &str) -> &Self::Output {
        self.get(index)
            .unwrap_or_else(|| panic!("{index} must exist"))
    }
}

#[derive(Debug, Clone)]
pub enum Match {
    Required,
    NoMatch,
    Match(HashMap<String, String>),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ArgType {
    key: Box<str>,
    kind: ArgKind,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ArgKind {
    Required,
    Optional,
    Variadic,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_args_good_input() {
        for input in [
            "!hello",
            "!hello world",
            "!hello <world>",
            "!hello <world?>",
            "!hello <world..>",
            "!hello <world> <test?>",
            "!hello <world> <test?> <rest..>",
        ] {
            let _: ExampleArgs = input.parse().unwrap();
        }
    }

    #[test]
    fn example_args_bad_input() {
        let bad_inputs = [
            ("", ExampleError::EmptyInput),
            (
                "<a?> <b>",
                ExampleError::OptionalBeforeRequired {
                    key: String::from("a"),
                },
            ),
            (
                "<a> <b?> <c>",
                ExampleError::OptionalBeforeRequired {
                    key: String::from("b"),
                },
            ),
            (
                "<a> <b..> <c?>",
                ExampleError::VariadicNotInTail {
                    key: String::from("b"),
                },
            ),
            (
                "<a> <b..> <c..>",
                ExampleError::MultipleVariadic {
                    keys: vec![String::from("b"), String::from("c")],
                },
            ),
            (
                "<a> <a..>",
                ExampleError::Duplicate {
                    key: String::from("a"),
                },
            ),
        ];

        for (input, error) in bad_inputs {
            let err = input
                .parse::<ExampleArgs>()
                .expect_err("this is a bad input");
            assert_eq!(err, error)
        }
    }
}
