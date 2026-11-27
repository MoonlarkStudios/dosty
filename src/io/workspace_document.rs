//! A document's save destination and the version its edits are based on.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Default)]
pub(crate) enum WorkspaceDocument {
    #[default]
    Unsaved,
    File {
        path: PathBuf,
        checkpoint: Option<String>,
    },
    Hosted(HostedDraft),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostedAddress {
    pub(crate) origin: String,
    pub(crate) id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HostedDraft {
    pub(crate) address: HostedAddress,
    pub(crate) revision: u64,
    pub(crate) saved_details: Details,
    pub(crate) details: Details,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Details {
    pub(crate) title: String,
    pub(crate) section: String,
    pub(crate) cover: Option<Cover>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cover {
    pub(crate) src: String,
    pub(crate) alt: String,
}
