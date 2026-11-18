use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// Malformed `.fta` / `.ped` input.
    Parse(String),
    /// A feature of the original OpenFTA that is not ported yet.
    Unsupported(String),
    /// A primary event has no probability available.
    MissingEvent(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Parse(m) => write!(f, "parse error: {m}"),
            Error::Unsupported(m) => write!(f, "unsupported: {m}"),
            Error::MissingEvent(id) => write!(f, "no probability for primary event `{id}`"),
        }
    }
}

impl std::error::Error for Error {}
