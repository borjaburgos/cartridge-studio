use serde::Serialize;
use serde_json::{json, Value};
use std::{fmt, io};

#[derive(Debug, Clone, Serialize)]
pub struct Error {
    #[serde(rename = "error")]
    pub code: String,
    pub message: String,
    pub action: String,
    pub details: Value,
    #[serde(skip)]
    pub exit_code: i32,
}
pub type Result<T> = std::result::Result<T, Error>;
impl Error {
    pub fn new(code: &str, message: impl Into<String>, action: &str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            action: action.into(),
            details: json!({}),
            exit_code: 2,
        }
    }
    pub fn details(mut self, details: Value) -> Self {
        self.details = details;
        self
    }
    pub fn exit(mut self, code: i32) -> Self {
        self.exit_code = code;
        self
    }
    pub fn check(message: impl Into<String>) -> Self {
        Self::new("CARTRIDGE_CHECK_FAILED", message, "Confirm the platform and physical board. Unplug USB before reseating the cartridge. Retain the report and backups before retrying.")
    }
    pub fn interrupted() -> Self {
        Self::new("INTERRUPTED", "Operation stopped. Completed backups and partial reads are retained.", "If erase or writing had started, reconnect and restore the saved source ROM. Verify the cartridge before using it.").exit(130)
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {}\nWhat to do: {}",
            self.code, self.message, self.action
        )
    }
}
impl std::error::Error for Error {}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        let (code, message, action) = match e.kind() {
            io::ErrorKind::NotFound => ("FILE_NOT_FOUND", "The selected file or folder is no longer available.", "Locate the file and load it again."),
            io::ErrorKind::PermissionDenied => ("FOLDER_ACCESS_DENIED", "This location cannot be read or written by your account.", "Choose a folder you own, or correct its access permissions."),
            io::ErrorKind::AlreadyExists => ("OUTPUT_EXISTS", "The output already exists.", "Choose a new path; existing backups are never overwritten."),
            _ if e.raw_os_error() == Some(libc::ENOSPC) => ("DISK_FULL", "The backup disk is full.", "Free space, retain the completed backups, and retry into a new folder."),
            _ => ("FILE_IO", "A file could not be read or saved.", "Check the path, available space, and permissions. Retain completed backups before retrying."),
        };
        Self::new(code, message, action).details(json!({"reason":e.to_string()}))
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::new(
            "INVALID_REQUEST",
            "The operation request or saved metadata is invalid.",
            "Restart the application and retry. Keep the diagnostic details if this repeats.",
        )
        .details(json!({"reason":e.to_string()}))
    }
}
