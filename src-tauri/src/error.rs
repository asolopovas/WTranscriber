use serde::{Serialize, Serializer};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("config: {0}")]
    Config(String),

    #[error("transcribe: {0}")]
    Transcribe(String),

    #[error("cancelled")]
    Cancelled,

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_to_human_readable_string() {
        let e = Error::Config("bad".into());
        let json = serde_json::to_string(&e).unwrap();
        assert_eq!(json, "\"config: bad\"");
    }

    #[test]
    fn cancelled_serializes_to_marker_string() {
        let json = serde_json::to_string(&Error::Cancelled).unwrap();
        assert_eq!(json, "\"cancelled\"");
    }

    #[test]
    fn from_io_error_round_trips_through_display() {
        let io = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let e: Error = io.into();
        assert!(e.to_string().starts_with("io:"));
    }

    #[test]
    fn from_serde_error_uses_serde_variant() {
        let bad: serde_json::Error = serde_json::from_str::<u32>("not-a-number").unwrap_err();
        let e: Error = bad.into();
        assert!(e.to_string().starts_with("serde:"));
    }

    #[test]
    fn from_anyhow_error_uses_other_variant() {
        let e: Error = anyhow::anyhow!("boom").into();
        assert!(e.to_string().contains("boom"));
    }
}
