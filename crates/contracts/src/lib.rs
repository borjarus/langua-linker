use serde::Serialize;

pub const API_VERSION: &str = "0.1";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfoDto {
    pub api_version: String,
    pub app_name: String,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorDto {
    pub code: String,
    pub message_key: String,
    pub retryable: bool,
}