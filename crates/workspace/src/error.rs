use std::fmt;

#[derive(Debug)]
pub struct WorkspaceError(pub String);

impl fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "workspace error: {}", self.0)
    }
}

impl std::error::Error for WorkspaceError {}
