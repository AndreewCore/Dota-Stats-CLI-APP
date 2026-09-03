use std::fmt;

#[derive(Debug)]
pub enum Error {
    Http(String),
    /// OpenDota's rate limit (HTTP 429). Split out from `Http` because it is the
    /// one network failure the user can act on: wait, or set an api_key.
    RateLimited,
    /// HTTP 404 — an account id that doesn't exist, or a private profile.
    NotFound,
    Parse(String),
    Io(std::io::Error),
    Config(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Http(m) => write!(f, "http error: {m}"),
            Error::RateLimited => write!(
                f,
                "OpenDota rate limit reached — wait a minute, or add an api_key to raise it"
            ),
            Error::NotFound => write!(f, "not found on OpenDota — check the account id, or the profile may be private"),
            Error::Parse(m) => write!(f, "parse error: {m}"),
            Error::Io(e) => write!(f, "io error: {e}"),
            Error::Config(m) => write!(f, "config error: {m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Parse(e.to_string())
    }
}

impl From<ureq::Error> for Error {
    fn from(e: ureq::Error) -> Self {
        match e {
            ureq::Error::Status(429, _) => Error::RateLimited,
            ureq::Error::Status(404, _) => Error::NotFound,
            ureq::Error::Status(code, r) => {
                Error::Http(format!("{code} {}", r.status_text()))
            }
            ureq::Error::Transport(t) => Error::Http(t.to_string()),
        }
    }
}
