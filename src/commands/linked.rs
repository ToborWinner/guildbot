use twilight_model::id::{marker::UserMarker, Id};

use super::{check_owner, CommandData, CommandError, CommandResponseBuilder, MissingRequirement};
use crate::{database::get_linked_user, ShardData};

type UserId = Id<UserMarker>;

make_command! {$
    [slash legacy]
    "linked"
    "Get information on the linking status of an user."
    |cd: std::time::Duration::from_secs(2),|
    (user, "user", "The Discord user."): <[user? UserId]>
}

pub(super) async fn handle(
    cmd: CommandData<'_>,
    data: &ShardData,
    args: Command,
) -> Result<(), CommandError> {
    let user_id = args.user.or_else(|| cmd.author_id()).ok_or(CommandError::Generic)?;
    let user_id = user_id.get().try_into().map_err(|_| CommandError::InvalidArgs("The user id you provided is too high.".to_string()))?;

    let linked = get_linked_user(&data.pool, user_id).await?;

    cmd.reply(
        data,
        CommandResponseBuilder::new()
            .ed(match linked {
                Some(user) => {
                    const BROKEN_ERR: &str = "Your linking status seems to be broken. Please use `unlink` and notify staff.";
                    let ign = user.ign.ok_or(CommandError::Specific(BROKEN_ERR.into()))?;
                    let uuid = user.uuid.ok_or(CommandError::Specific(BROKEN_ERR.into()))?;
                    format!("**User:** <@{}> (`{}`)\n**IGN:** `{}`\n**UUID:** `{}`", user_id, user_id, ign, uuid)
                },
                None => format!("**User:** <@{}> (`{}`)\n**IGN:** -\n**UUID:** -", user_id, user_id)
            })
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
        check_owner(cmd).map_err(|_| MissingRequirement::General("You don't have the required permission to check someone else's linking status.".to_string()))
    } else {
        Ok(())
    }
}
