use serde::{Deserialize, Serialize};

/// Media metadata response for MSC2380 unstable media info endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MSC2380MediaInfo {
    pub content_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u64>,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub thumbnails: Vec<MediaThumbnailInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaThumbnailInfo {
    pub width: u64,
    pub height: u64,
    pub ready: bool,
}

pub type MediaInfo = MSC2380MediaInfo;
