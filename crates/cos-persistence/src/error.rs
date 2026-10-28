use thiserror::Error;

#[derive(Error, Debug)]
pub enum PersistenceError {
    #[error("I/O error during persistence operation: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization / JSON error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Record not found: {0}")]
    NotFound(String),

    #[error("Concurrency or conflict error: {0}")]
    Conflict(String),

    #[error("Storage error: {0}")]
    StorageError(String),
}
