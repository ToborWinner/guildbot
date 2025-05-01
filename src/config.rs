use std::{collections::HashMap, fs, path::Path};

use serde::Deserialize;
use twilight_model::id::{
    marker::{ChannelMarker, GuildMarker, RoleMarker, UserMarker},
    Id,
};

#[derive(Clone, Deserialize)]
pub struct BotConfig {
    pub token: String,
    pub database_url: String,
    pub api_key: String,

    pub guild_id: Id<GuildMarker>,
    pub verified_role_id: Id<RoleMarker>,
    pub nick_bypass_role: Id<RoleMarker>,
    pub notification_channel: Id<ChannelMarker>,
    pub notification_mention: String,
    pub no_permission: Id<UserMarker>,
    pub guilds: Vec<BotConfigGuild>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BotConfigGuild {
    pub name: String,
    pub role: Id<RoleMarker>,
    pub ranks: HashMap<String, Id<RoleMarker>>,
}

#[derive(Debug, thiserror::Error)]
pub enum BotConfigParseError {
    #[error("There was an error while reading the JSON file: {0}")]
    ReadFile(#[from] std::io::Error),

    #[error("There was an error while parsing the JSON file: {0}")]
    ParseJSON(#[from] serde_json::Error),
}

impl BotConfig {
    pub fn from_json(path: impl AsRef<Path>) -> Result<Self, BotConfigParseError> {
        let file_content = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&file_content)?)
    }
}
