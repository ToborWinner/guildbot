use std::time::Duration;

use twilight_model::guild::Member;

use super::{check_owner, CommandData, CommandError, CommandResponseBuilder, MissingRequirement};
use crate::{database::{get_linked_user, insert_linked_user}, minecraft::{get_guild_members, get_user_id, LinkedDiscordRequestError, MinecraftUserRequestError, UserIdResponse}, ShardData};

make_command! {$
    [slash legacy]
    "masslink"
    "Link all users with the Verified role by checking their nickname."
    |cd: std::time::Duration::from_secs(600),|
}

pub(super) async fn handle(
    cmd: CommandData<'_>,
    data: &ShardData,
    _args: Command,
) -> Result<(), CommandError> {
    let members = get_guild_members(data).await.map_err(|e| {
        tracing::error!("Failed to fetch guild members for masslink: {}", e);
        CommandError::Specific("Failed to fetch guild members.".into())
    })?;

    let members: Vec<Member> = members.into_iter().filter(|x| x.roles.contains(&data.config.verified_role_id)).collect();

    const DELAY: Duration = Duration::from_secs(10);

    cmd.reply(
        data,
        CommandResponseBuilder::new()
            .etd("Masslink", format!("There are {} members with the verified role.\n**Approximate time needed:** `{:?}`.", members.len(), DELAY * members.len().try_into().map_err(|_| CommandError::Generic)?))
            .build(),
    )
    .await?;

    let mut successful = 0;
    let mut already = 0;

    for (index, member) in members.iter().enumerate() {
        let user_id_with_marker = member.user.id;
        let user_id = member.user.id.get().try_into().map_err(|_| CommandError::Specific(format!["An user id is too high: <@{}> (`{}`).", user_id_with_marker.get(), user_id_with_marker.get()]))?;

        if get_linked_user(&data.pool, user_id).await?.is_some() {
            tracing::info!("Masslink: {}/{} (skipped: already linked)", index, members.len());
            already += 1;
            continue;
        }

        tokio::time::sleep(DELAY).await;
        tracing::info!("Masslink: {}/{}", index, members.len());

        let potential_ign = member.nick.as_ref().or(member.user.global_name.as_ref()).unwrap_or(&member.user.name);

        let UserIdResponse { id, name } = match get_user_id(&data.reqwest_client, potential_ign).await {
            Err(MinecraftUserRequestError::InvalidName) => continue,
            Err(MinecraftUserRequestError::UserNotFound) => continue,
            Err(err) => {
                tracing::error!("Error while fetching minecraft id: {:?}", err);
                return Err(CommandError::Specific(format!["Error while fetching minecraft UUID of user <@{}> (`{}`).", user_id, user_id]));
            },
            Ok(x) => x,
        };

        let linked_discord = match data.hypixel.get_linked_discord(&id).await {
            Err(LinkedDiscordRequestError::UserNotFound) => continue,
            Err(LinkedDiscordRequestError::UserNotLinked) => continue,
            Err(err) => {
                tracing::error!("Error while fetching Discord linked user: {:?}", err);
                return Err(CommandError::Specific(format!["Error while fetching linked username of user <@{}> (`{}`).", user_id, user_id]));
            },
            Ok(x) => x,
        };

        if linked_discord != member.user.name {
            continue;
        }

        if insert_linked_user(&data.pool, user_id, &name, &id).await? != 1 {
            return Err(CommandError::Specific(format!["Error while inserting data in database for user <@{}> (`{}`)", user_id, user_id]));
        }
        successful += 1;
        tracing::info!("Successfully linked user {} to name `{}`", user_id, name);
    }

    cmd.followup(
        data,
        CommandResponseBuilder::new()
            .etd("Masslink complete", format!("**Already linked:** `{}`\n**Successful:** `{}`\n**Failed:** `{}`\n**Total:** `{}`", already, successful, members.len() - successful - already, members.len()))
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
