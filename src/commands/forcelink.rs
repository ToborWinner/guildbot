use twilight_model::id::{marker::UserMarker, Id};

use super::{check_owner, CommandData, CommandError, CommandResponseBuilder, MissingRequirement};
use crate::{database::{get_linked_user, insert_linked_user}, minecraft::{get_user_id, MinecraftUserRequestError, UserIdResponse}, ShardData};

type UserId = Id<UserMarker>;

make_command! {$
    [slash legacy]
    "forcelink"
    "Force link an user to an IGN."
    |cd: std::time::Duration::from_secs(2),|
    (ign, "ign", "The IGN of the user to force link."): <[word String] max: 16; min: 2;>
    (user, "user", "The Discord user to force link."): <[user? UserId]>
}

pub(super) async fn handle(
    cmd: CommandData<'_>,
    data: &ShardData,
    args: Command,
) -> Result<(), CommandError> {
    let user_id_with_marker = args.user.or_else(|| cmd.author_id()).ok_or(CommandError::Generic)?;
    let user_id = user_id_with_marker.get().try_into().map_err(|_| CommandError::InvalidArgs("The user id you provided is too high.".to_string()))?;
    let guild_id = cmd.guild_id().ok_or(CommandError::Specific("This command cannot be used outside of the guild.".to_string()))?;

    if let Some(user) = get_linked_user(&data.pool, user_id).await? {
        let ign = user.ign.ok_or(CommandError::Specific("Your linking status seems to be broken. Please use `unlink` and notify staff.".into()))?;
        return Err(CommandError::Specific(format!("The user <@{}> (`{}`) is already linked to `{}`. You can unlink them using `unlink <user>`.", user_id, user_id, ign)))
    }

    let UserIdResponse { id, name } = get_user_id(&data.reqwest_client, &args.ign).await.map_err(|e| match e {
        MinecraftUserRequestError::InvalidName => CommandError::Specific("The name you provided contains characters that are not valid in a Minecraft username.".into()),
        MinecraftUserRequestError::UserNotFound => CommandError::Specific(format!("There doesn't seem to exist a Minecraft user with the name `{}`.", args.ign)),
        err => {
            tracing::error!("Error while fetching minecraft id: {:?}", err);
            CommandError::Generic
        }
    })?;

    if insert_linked_user(&data.pool, user_id, &name, &id).await? != 1 {
        return Err(CommandError::Generic);
    }

    _ = data.client.add_guild_member_role(guild_id, user_id_with_marker, data.config.verified_role_id).await.map_err(|_| CommandError::Specific("Failed to add the Verified role to the user. Please contact staff.".to_string()))?;

    cmd.reply(
        data,
        CommandResponseBuilder::new()
            .ed(format!("Forcefully linked user <@{}> (`{}`) to IGN `{}` (`{}`).", user_id, user_id, name, id))
            .build(),
    )
    .await
}

pub(super) async fn check_gen_requirements(
    cmd: CommandData<'_>,
    _data: &ShardData,
) -> Result<(), MissingRequirement> {
    check_owner(cmd)
}

no_req!();
