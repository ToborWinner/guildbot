use twilight_http::response::StatusCode;
use twilight_model::id::{marker::{GuildMarker, UserMarker}, Id};

use super::{check_owner, CommandData, CommandError, CommandResponseBuilder, MissingRequirement};
use crate::{database::{get_linked_user, insert_linked_user}, minecraft::{get_user_id, LinkedDiscordRequestError, MinecraftUserRequestError, UserIdResponse}, ShardData};

type UserId = Id<UserMarker>;

macro_rules! steps {
    () => {
        "**1.** Log on Hypixel and go to a lobby using `/lobby`. Ensure you are in the `ALL` chat by running `/chat a`.\n**2.** Run the `/profile` command and click on the `Social Media` item (third row, fourth column).\n**3.** Left click on the `Discord` item and type your Discord username in chat (it won't send it).\n**4.** Close the menu that Hypixel re-opened.\n**5.** Come back here and run the `link` command again."
    }
}

make_command! {$
    [slash legacy]
    "link"
    "Link an user to an IGN."
    |cd: std::time::Duration::from_secs(2),|
    (ign, "ign", "The IGN to link to."): <[word String] max: 16; min: 2;>
    (user, "user", "The Discord user to link. Requires staff."): <[user? UserId]>
}

pub(super) async fn handle(
    cmd: CommandData<'_>,
    data: &ShardData,
    args: Command,
) -> Result<(), CommandError> {
    let sender_id = cmd.author_id().ok_or(CommandError::Generic)?;
    let user_id_with_marker = args.user.unwrap_or(sender_id);
    let user_id: i64 = user_id_with_marker.get().try_into().map_err(|_| CommandError::InvalidArgs("The user id you provided is too high.".to_string()))?;
    let guild_id = cmd.guild_id().ok_or(CommandError::Specific("This command cannot be used outside of the guild.".to_string()))?;

    let username = if sender_id == user_id_with_marker {
        match cmd {
            CommandData::ChatInput(data) => data.user.as_ref().expect("Already checked").name.clone(),
            CommandData::Legacy(data) => data.author.name.clone(),
        }
    } else {
        // TODO: Fix cache and add result of this to cache as well
        let cached = data.cache.user(user_id_with_marker);
        match cached {
            Some(user) => user.name.clone(),
            None => {
                let res = data.client.user(user_id_with_marker).await.map_err(|e| match e.kind() {
                    twilight_http::error::ErrorType::Response { status: StatusCode::NOT_FOUND, .. } => CommandError::Specific(format!("An user with id `{}` doesn't seem to exist.", user_id)),
                    _ => CommandError::Generic,
                })?;
                let status = res.status();
                if status == StatusCode::NOT_FOUND {
                    return Err(CommandError::Specific(format!("An user with id `{}` doesn't seem to exist.", user_id)));
                } else if status != StatusCode::OK {
                    return Err(CommandError::Generic);
                }
                res.model().await.map_err(|_| CommandError::Generic)?.name
            }
        }
    };

    let builder = link_user(user_id_with_marker, guild_id, &args.ign, &username, data).await?;

    cmd.reply(data, builder.build()).await
}

no_gen_req!();

pub(super) async fn check_requirements(
    cmd: CommandData<'_>,
    _data: &ShardData,
    args: &Command,
) -> Result<(), MissingRequirement> {
    if args.user.is_some() && args.user != cmd.author_id() {
        check_owner(cmd).map_err(|_| MissingRequirement::General("You don't have the required permission to link someone else.".to_string()))
    } else {
        Ok(())
    }
}

pub(super) async fn link_user(user_id_with_marker: Id<UserMarker>, guild_id: Id<GuildMarker>, ign_input: &str, username: &str, data: &ShardData) -> Result<CommandResponseBuilder, CommandError> {
    let user_id = user_id_with_marker.get().try_into().map_err(|_| CommandError::InvalidArgs("The user id you provided is too high.".to_string()))?;

    if let Some(user) = get_linked_user(&data.pool, user_id).await? {
        let ign = user.ign.ok_or(CommandError::Specific("Your linking status seems to be broken. Please use `unlink` and notify staff.".into()))?;
        return Err(CommandError::Specific(format!("The user <@{}> (`{}`) is already linked to `{}`. You can unlink with `unlink`. Unlinking is required to link to a different IGN.", user_id, user_id, ign)))
    }

    let UserIdResponse { id, name } = get_user_id(&data.reqwest_client, ign_input).await.map_err(|e| match e {
        MinecraftUserRequestError::InvalidName => CommandError::Specific("The name you provided contains characters that are not valid in a Minecraft username.".into()),
        MinecraftUserRequestError::UserNotFound => CommandError::Specific(format!("There doesn't seem to exist a Minecraft user with the name `{}`.", ign_input)),
        err => {
            tracing::error!("Error while fetching minecraft id: {:?}", err);
            CommandError::Generic
        }
    })?;

    let linked_discord = data.hypixel.get_linked_discord(&id).await.map_err(|e| match e {
        LinkedDiscordRequestError::UserNotFound => CommandError::Specific(format!("The Minecraft user `{}` (`{}`) seems to have never played on Hypixel.", name, id)),
        LinkedDiscordRequestError::UserNotLinked => CommandError::Specific(format!(concat!["The Hypixel player `{}` (`{}`) doesn't have their Discord account linked on the Hypixel server.\n\n> **Steps to link:**\n", steps!()], name, id)),
        err => {
            tracing::error!("Error while fetching Discord linked user: {:?}", err);
            CommandError::Generic
        }
    })?;

    if *username != linked_discord {
        return Err(CommandError::Specific(format!(concat!["The username linked on Hypixel and your current Discord username don't seem to match.\n**Your Username:** `{}`\n**Username linked on Hypixel:** `{}`\n\n> **Please update the username on Hypixel:**\n", steps!()], username, linked_discord)))
    }

    if insert_linked_user(&data.pool, user_id, &name, &id).await? != 1 {
        return Err(CommandError::Generic);
    }

    _ = data.client.add_guild_member_role(guild_id, user_id_with_marker, data.config.verified_role_id).await.map_err(|_| CommandError::Specific("Failed to add the Verified role to the user. Please contact staff.".to_string()))?;

    Ok(CommandResponseBuilder::new()
        .ed(format!("Linked user <@{}> (`{}`) to IGN `{}` (`{}`)!", user_id, user_id, name, id)))
}
