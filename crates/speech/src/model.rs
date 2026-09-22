//! Speech model resolution, downloading, and integrity verification.
//!
//! Models are stored in the state directory under `models/` with mode 0700 for directories
//! and 0600 for files on Unix systems, in accordance with `STATE-1`. Downloads are routed
//! through `bravebot-net` in accordance with `NET-1`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Default model name for English speech recognition.
pub const DEFAULT_VOSK_MODEL_NAME: &str = "vosk-model-small-en-us-0.15";

/// Known expected SHA-256 digest of the default model archive.
pub const DEFAULT_MODEL_SHA256: &str =
    "4d47eb38b939f60f64fb5625ff11eb85a73e662da3c6046e0b74fbbf4f48ad42";

/// Error type for speech model management.
#[derive(Debug)]
pub enum ModelError {
    NotFound(PathBuf),
    Io(io::Error),
    DigestMismatch { expected: String, actual: String },
    Network(String),
}

impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(p) => write!(f, "model directory not found at {}", p.display()),
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::DigestMismatch { expected, actual } => {
                write!(f, "digest mismatch: expected {expected}, got {actual}")
            }
            Self::Network(e) => write!(f, "network error while fetching model: {e}"),
        }
    }
}

impl std::error::Error for ModelError {}

impl From<io::Error> for ModelError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

/// Resolves the model path, checking user override or default state directory.
pub fn resolve_model_path(override_path: Option<&Path>) -> Result<PathBuf, ModelError> {
    if let Some(p) = override_path {
        if p.is_dir() {
            return Ok(p.to_path_buf());
        }
        return Err(ModelError::NotFound(p.to_path_buf()));
    }

    let base = default_model_directory()?;
    let model_dir = base.join(DEFAULT_VOSK_MODEL_NAME);
    if model_dir.is_dir() {
        Ok(model_dir)
    } else {
        Err(ModelError::NotFound(model_dir))
    }
}

/// Default model storage directory under `~/.bravebot/models`.
pub fn default_model_directory() -> Result<PathBuf, io::Error> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "neither HOME nor USERPROFILE set"))?;
    let path = PathBuf::from(home).join(".bravebot").join("models");
    ensure_secure_dir(&path)?;
    Ok(path)
}

/// Ensures directory exists with restricted permissions (0700 on Unix) per STATE-1.
pub fn ensure_secure_dir(dir: &Path) -> Result<(), io::Error> {
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Verifies that bytes match the expected SHA-256 hex digest.
pub fn verify_digest(bytes: &[u8], expected_hex: &str) -> Result<(), ModelError> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let result = hasher.finalize();
    let actual_hex = format!("{:x}", result);

    if actual_hex.eq_ignore_ascii_case(expected_hex) {
        Ok(())
    } else {
        Err(ModelError::DigestMismatch {
            expected: expected_hex.to_string(),
            actual: actual_hex,
        })
    }
}
