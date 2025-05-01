use std::{fmt::Display, sync::Arc, time::Duration};

use reqwest::{Client, StatusCode};
use serde::Deserialize;
use twilight_http::response::DeserializeBodyError;
use twilight_model::{
    guild::Member,
    id::{
        marker::{RoleMarker, UserMarker},
        Id,
    },
};

use crate::{
    database::{
        get_linked_users_by_ids, get_linked_users_by_uuids, update_linked_user, DatabaseError,
    },
    ShardData,
};

#[derive(Deserialize, Debug)]
pub struct GuildResponse {
    success: bool,
    guild: Option<HypixelGuild>,
}

#[derive(Deserialize, Debug)]
pub struct HypixelGuild {
    pub members: Vec<HypixelGuildMember>,
}

#[derive(Deserialize, Debug)]
pub struct HypixelGuildMember {
    pub uuid: String,
    pub rank: String,
}

#[derive(Deserialize, Debug)]
pub struct UserIdResponse {
    pub id: String,
    pub name: String,
}

#[derive(Deserialize, Debug)]
struct LinkedDiscordResponse {
    success: bool,
    player: Option<LinkedDiscordResponsePlayer>,
}

#[derive(Deserialize, Debug)]
struct LinkedDiscordResponsePlayer {
    #[serde(rename = "socialMedia")]
    social_media: Option<LinkedDiscordResponsePlayerSocialMedia>,
}

#[derive(Deserialize, Debug)]
struct LinkedDiscordResponsePlayerSocialMedia {
    links: Option<LinkedDiscordResponsePlayerSocialMediaLinks>,
}

#[derive(Deserialize, Debug)]
struct LinkedDiscordResponsePlayerSocialMediaLinks {
    #[serde(rename = "DISCORD")]
    discord: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum MinecraftUserRequestError {
    #[error("The status code is not 200")]
    BadStatusCode(StatusCode),

    #[error("There was a reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("No user with the given name.")]
    UserNotFound,

    #[error("The name contains invalid characters.")]
    InvalidName,
}

#[derive(Debug, thiserror::Error)]
pub enum LinkedDiscordRequestError {
    #[error("The status code is not 200")]
    BadStatusCode(StatusCode),

    #[error("There was a reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("No user with the given name.")]
    UserNotFound,

    #[error("No linked Discord account.")]
    UserNotLinked,
}

#[derive(Debug, thiserror::Error)]
pub enum GuildRequestError {
    #[error("The status code is not 200")]
    BadStatusCode(StatusCode),

    #[error("There was a reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("No guild with the given name.")]
    GuildNotFound,
}

#[derive(Debug, thiserror::Error)]
pub enum GetGuildMembersError {
    #[error("ModelError: {0}")]
    ModelError(#[from] DeserializeBodyError),

    #[error("HttpError: {0}")]
    HttpError(#[from] twilight_http::Error),

    #[error("There are too many members in the guild.")]
    TooManyMembers,
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateRolesError {
    #[error("GuildRequestError: {0}")]
    GuildRequest(#[from] GuildRequestError),

    #[error("DatabaseError: {0}")]
    Database(#[from] DatabaseError),

    #[error("GetGuildMembersError: {0}")]
    GetGuildMembers(#[from] GetGuildMembersError),

    #[error("InvalidDbUser: {0}")]
    InvalidDbUser(i64),
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateUsersError {
    #[error("UpdateRolesError: {0}")]
    UpdateRoles(#[from] UpdateRolesError),

    #[error("Error updating member: {0}")]
    UpdateMember(#[from] twilight_http::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateIgnsError {
    #[error("DatabaseError: {0}")]
    Database(#[from] DatabaseError),

    #[error("GetGuildMembersError: {0}")]
    GetGuildMembers(#[from] GetGuildMembersError),

    #[error("InvalidDbUser: {0}")]
    InvalidDbUser(i64),

    #[error("MinecraftUserRequestError: {0}")]
    MinecraftUserRequestError(#[from] MinecraftUserRequestError),
}

pub async fn get_user_id(
    client: &Client,
    name: &str,
) -> Result<UserIdResponse, MinecraftUserRequestError> {
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || name.len() > 16 {
        return Err(MinecraftUserRequestError::InvalidName);
    }

    let response = client
        .get("https://api.mojang.com/users/profiles/minecraft/".to_owned() + name)
        .send()
        .await?;

    let status = response.status();

    match status {
        StatusCode::NOT_FOUND => return Err(MinecraftUserRequestError::UserNotFound),
        StatusCode::OK => (),
        code => return Err(MinecraftUserRequestError::BadStatusCode(code)),
    }

    Ok(response.json().await?)
}

pub async fn get_user_ign(
    client: &Client,
    uuid: &str,
) -> Result<UserIdResponse, MinecraftUserRequestError> {
    if !uuid.chars().all(|c| c.is_ascii_alphanumeric()) || uuid.len() > 36 {
        return Err(MinecraftUserRequestError::InvalidName);
    }

    let response = client
        .get("https://api.minecraftservices.com/minecraft/profile/lookup/".to_owned() + uuid)
        .send()
        .await?;

    let status = response.status();

    match status {
        StatusCode::NOT_FOUND => return Err(MinecraftUserRequestError::UserNotFound),
        StatusCode::OK => (),
        code => return Err(MinecraftUserRequestError::BadStatusCode(code)),
    }

    Ok(response.json().await?)
}

pub struct HypixelAPI {
    api_key: String,
    client: Client,
}

// TODO: Handle rate limits (check headers)
impl HypixelAPI {
    pub fn new(api_key: impl Into<String>, client: Client) -> Self {
        Self {
            api_key: api_key.into(),
            client,
        }
    }

    pub async fn get_linked_discord(
        &self,
        uuid: &str,
    ) -> Result<String, LinkedDiscordRequestError> {
        let response = self
            .client
            .get("https://api.hypixel.net/v2/player?uuid=".to_owned() + uuid)
            .header("API-Key", &self.api_key)
            .send()
            .await?;

        let status = response.status();

        if status != StatusCode::OK {
            return Err(LinkedDiscordRequestError::BadStatusCode(status));
        }

        let parsed: LinkedDiscordResponse = response.json().await?;

        if !parsed.success {
            return Err(LinkedDiscordRequestError::UserNotFound);
        }

        parsed
            .player
            .and_then(|x| x.social_media)
            .and_then(|x| x.links)
            .and_then(|x| x.discord)
            .ok_or(LinkedDiscordRequestError::UserNotLinked)
    }

    pub async fn get_guild(&self, name: &str) -> Result<HypixelGuild, GuildRequestError> {
        let response = self
            .client
            .get("https://api.hypixel.net/v2/guild?name=".to_owned() + name)
            .header("API-Key", &self.api_key)
            .send()
            .await?;

        let status = response.status();

        if status != StatusCode::OK {
            return Err(GuildRequestError::BadStatusCode(status));
        }

        let parsed: GuildResponse = response.json().await?;

        if !parsed.success || parsed.guild.is_none() {
            return Err(GuildRequestError::GuildNotFound);
        }

        Ok(parsed.guild.expect("Already checked"))
    }
}

pub async fn get_guild_members(data: &ShardData) -> Result<Vec<Member>, GetGuildMembersError> {
    let mut members = Vec::new();
    let mut last_user: Option<Id<UserMarker>> = None;
    loop {
        if members.len() >= 5000 {
            return Err(GetGuildMembersError::TooManyMembers);
        }

        let mut req = data.client.guild_members(data.config.guild_id).limit(1000);
        if let Some(id) = last_user {
            req = req.after(id);
        }

        let res = req.await?.models().await?;

        if res.len() != 1000 {
            members.extend(res);
            break;
        }

        last_user = res.last().map(|member| member.user.id);
        if last_user.is_none() {
            break;
        }

        members.extend(res);
    }

    Ok(members)
}

pub struct MemberUpdate {
    id: Id<UserMarker>,
    roles: Option<Vec<Id<RoleMarker>>>,
    add: Vec<Id<RoleMarker>>,
    remove: Vec<Id<RoleMarker>>,
    nick: Option<String>,
}

impl MemberUpdate {
    fn is_empty(&self) -> bool {
        self.roles.is_none() && self.nick.is_none()
    }
}

impl Display for MemberUpdate {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if self.is_empty() {
            return write!(f, "<@{}>: No updates.", self.id);
        }
        let mut output = String::new();
        if !self.add.is_empty() {
            output += &format![
                "\n - Add role{} {}.",
                if self.add.len() == 1 { "" } else { "s" },
                self.add
                    .iter()
                    .map(|x| format!["<@&{}>", x])
                    .collect::<Vec<String>>()
                    .join(", ")
            ];
        }
        if !self.remove.is_empty() {
            output += &format![
                "\n- Remove role{} {}.",
                if self.remove.len() == 1 { "" } else { "s" },
                self.remove
                    .iter()
                    .map(|x| format!["<@&{}>", x])
                    .collect::<Vec<String>>()
                    .join(", ")
            ];
        }
        if let Some(nick) = &self.nick {
            output += &format!["\n- Change nickname to `{}`.", nick];
        }
        write!(f, "<@{}>:{}", self.id, output)
    }
}

pub async fn get_updates(data: &ShardData) -> Result<Vec<MemberUpdate>, UpdateRolesError> {
    let mut guilds = Vec::with_capacity(data.config.guilds.len());

    for guild in data.config.guilds.iter() {
        guilds.push((guild, data.hypixel.get_guild(&guild.name).await?.members));
    }

    let list_of_uuids: Vec<&String> = guilds
        .iter()
        .flat_map(|(_, v)| v.iter())
        .map(|x| &x.uuid)
        .collect();
    let linked_users = get_linked_users_by_uuids(&data.pool, list_of_uuids).await?;

    let guild_members: Vec<Member> = get_guild_members(data).await?;

    let mut updates = Vec::new();

    for member in guild_members {
        let linked = linked_users
            .iter()
            .find(|x| x.id > 0 && x.id as u64 == member.user.id.get());

        let mut to_remove = Vec::new();
        let mut to_add = Vec::new();
        let mut new_nick: Option<String> = None;

        if let Some(dbuser) = linked {
            let uuid = dbuser
                .uuid
                .as_ref()
                .ok_or(UpdateRolesError::InvalidDbUser(dbuser.id))?;
            let ign = dbuser
                .ign
                .as_ref()
                .ok_or(UpdateRolesError::InvalidDbUser(dbuser.id))?;

            to_add.push(data.config.verified_role_id);
            if member
                .roles
                .iter()
                .all(|x| *x != data.config.nick_bypass_role)
                && ((member.nick.is_some() && member.nick.as_ref().unwrap() != ign)
                    || (member.nick.is_none()
                        && member.user.global_name.unwrap_or(member.user.name) != *ign))
            {
                new_nick = Some(ign.clone());
            }

            let data = guilds
                .iter()
                .find_map(|(g, m)| m.iter().find(|x| x.uuid == *uuid).map(|x| (x, *g)));

            if let Some((guild_member, guild_config)) = data {
                to_add.push(guild_config.role);
                let role = guild_config.ranks.get(&guild_member.rank);
                let all_roles = guild_config.ranks.values();
                if let Some(role) = role {
                    to_add.push(*role);
                    to_remove.extend(all_roles.filter(|x| **x != *role));
                } else {
                    to_remove.extend(all_roles);
                }
            }
        } else {
            to_remove.reserve(guilds.len() + 1);
            to_remove.extend(guilds.iter().map(|(c, _)| c.role));
            to_remove.push(data.config.verified_role_id);
        }

        let added: Vec<Id<RoleMarker>> = to_add
            .iter()
            .copied()
            .filter(|x| !member.roles.contains(x))
            .collect();
        let removed: Vec<Id<RoleMarker>> = member
            .roles
            .iter()
            .copied()
            .filter(|x| to_remove.contains(x))
            .collect();

        if new_nick.is_none() && removed.is_empty() && added.is_empty() {
            continue;
        }

        let roles = member
            .roles
            .iter()
            .copied()
            .filter(|x| !to_remove.contains(x))
            .chain(added.iter().copied())
            .collect();

        updates.push(MemberUpdate {
            id: member.user.id,
            roles: if added.is_empty() && removed.is_empty() {
                None
            } else {
                Some(roles)
            },
            add: added,
            remove: removed,
            nick: new_nick,
        });
    }

    Ok(updates)
}

pub async fn update_igns(data: &ShardData) -> Result<(), UpdateIgnsError> {
    let guild_members = get_guild_members(data).await?;
    let linked_users = get_linked_users_by_ids(
        &data.pool,
        guild_members
            .into_iter()
            .filter(|x| {
                x.user.id.get() < i64::MAX as u64 && x.roles.contains(&data.config.verified_role_id)
            })
            .map(|x| x.user.id.get() as i64)
            .collect(),
    )
    .await?;

    tracing::info!("Checking IGNs for {} users.", linked_users.len());

    for user in linked_users {
        tracing::info!("Checking IGN for user {}.", user.id);
        let uuid = user.uuid.ok_or(UpdateIgnsError::InvalidDbUser(user.id))?;
        let mc = get_user_ign(&data.reqwest_client, &uuid).await?;
        if user.ign.is_none() || mc.name != user.ign.unwrap() {
            tracing::info!(
                "Updating IGN for user `{}`. New IGN: `{}`.",
                user.id,
                mc.name
            );
            update_linked_user(&data.pool, user.id, mc.name).await?;
        }
        tokio::time::sleep(Duration::from_secs(10)).await;
    }

    Ok(())
}

pub async fn update_users(data: &ShardData) -> Result<(), UpdateUsersError> {
    let updates = get_updates(data).await?;

    tracing::info!("Running {} member updates.", updates.len());

    for update in updates {
        if update.is_empty() {
            tracing::warn!("Update of member {} was empty!", update.id);
            continue;
        }
        tracing::info!("Updating member {}.", update.id);
        let mut req = data
            .client
            .update_guild_member(data.config.guild_id, update.id);
        if let Some(nick) = &update.nick {
            if update.id != data.config.no_permission {
                req = req.nick(Some(nick));
            } else {
                tracing::info!("Skipping nick update for member {}.", update.id);
            }
        }
        if let Some(roles) = &update.roles {
            req = req.roles(roles);
        }
        _ = req.await?;
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    Ok(())
}

pub async fn send_notification(data: &ShardData, notification: &str) {
    _ = data
        .client
        .create_message(data.config.notification_channel)
        .content(&format![
            "{}, {}",
            data.config.notification_mention, notification
        ])
        .await;
}

pub async fn update_loop(data: Arc<ShardData>) {
    loop {
        tokio::time::sleep(Duration::from_secs(1800)).await;
        if data.paused_loop.load(std::sync::atomic::Ordering::SeqCst) {
            continue;
        }
        if let Err(e) = update_igns(&data).await {
            tracing::error!("Error while updating IGNs: {}", e);
            send_notification(
                &data,
                "there was an error in the update loop while getting IGNs.",
            )
            .await;
            continue;
        }
        for i in 0..4 {
            if data.paused_loop.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            if let Err(e) = update_users(&data).await {
                tracing::error!("Error while updating members: {}", e);
                send_notification(
                    &data,
                    "there was an error in the update loop while updating members.",
                )
                .await;
            }
            if i != 3 {
                tokio::time::sleep(Duration::from_secs(1800)).await;
            }
        }
    }
}
