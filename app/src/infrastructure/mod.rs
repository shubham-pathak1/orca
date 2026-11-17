//! App-local persistence, channels, watching and error presentation; domain workflows live in orca-services.
pub(crate) mod errors;
pub(crate) mod jobs;
pub(crate) mod operation_dispatch;
pub(crate) mod persistence;
pub(crate) mod watcher;
