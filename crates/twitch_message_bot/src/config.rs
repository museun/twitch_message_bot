use std::time::Duration;

#[non_exhaustive]
pub struct Config {
    pub(crate) name: String,
    pub(crate) token: String,
    pub(crate) ping_delay: Duration,
}

impl Config {
    pub fn new(name: impl ToString, token: impl ToString) -> Self {
        Self {
            name: name.to_string(),
            token: token.to_string(),
            ping_delay: Duration::from_secs(30),
        }
    }

    pub fn with_ping_delay(self, delay: impl Into<Duration>) -> Self {
        Self {
            ping_delay: delay.into(),
            ..self
        }
    }
}
