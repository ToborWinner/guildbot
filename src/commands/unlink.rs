use twilight_model::id::{marker::UserMarker, Id};

use super::{check_owner, CommandData, CommandError, CommandResponseBuilder, MissingRequirement};
use crate::{database::{delete_linked_user, get_linked_user}, ShardData};

type UserId = Id<UserMarker>;

make_command! {$
    [slash legacy]
    "unlink"
    "Unlink your IGN from your user."
    |cd: std::time::Duration::from_secs(2),|
    (user, "user", "The Discord user to unlink. Requires staff."): <[user? UserId]>
}

pub(super) async fn handle(
    cmd: CommandData<'_>,
    data: &ShardData,
    args: Command,
) -> Result<(), CommandError> {
    let user_id_with_marker = args.user.or_else(|| cmd.author_id()).ok_or(CommandError::Generic)?;
    let user_id = user_id_with_marker.get().try_into().map_err(|_| CommandError::InvalidArgs("The user id you provided is too high.".to_string()))?;
    let guild_id = cmd.guild_id().ok_or(CommandError::Specific("This command cannot be used outside of the guild.".to_string()))?;

    if get_linked_user(&data.pool, user_id).await?.is_none() {
        return Err(CommandError::Specific("You are not linked! Run `link` to link your IGN to your user. This command is to unlink someone who is already linked.".to_string()))
    }

    if delete_linked_user(&data.pool, user_id).await? != 1 {
        return Err(CommandError::Generic);
    }

    _ = data.client.remove_guild_member_role(guild_id, user_id_with_marker, data.config.verified_role_id).await.map_err(|_| CommandError::Specific("Failed to remove the Verified role from the user. Please contact staff.".to_string()))?;

    cmd.reply(
        data,
        CommandResponseBuilder::new()
            .ed(format!("<@{}> (`{}`) has been successfully unlinked!", user_id, user_id))
            .build(),
    )
    .await
}

no_gen_req!();

pub(super) async fn check_requirements(
    cmd: CommandData<'_>,
    _data: &ShardData,
    args: &Command,
) -> Result<(), MissingRequirement> {
    if args.user.is_some() && args.user != cmd.author_id() {
        check_owner(cmd).map_err(|_| MissingRequirement::General("You don't have the required permission to unlink someone else.".to_string()))
    } else {
        Ok(())
    }
}
