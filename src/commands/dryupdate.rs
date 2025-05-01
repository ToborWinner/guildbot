use std::time::Duration;

use super::{check_owner, make_descless_embed, make_empty_embed, CommandData, CommandError, CommandResponseBuilder, MissingRequirement};
use crate::{minecraft::{get_updates, UpdateRolesError}, ShardData};

make_command! {$
    [slash legacy]
    "dryupdate"
    "Shows the changes that an update would bring."
    |cd: std::time::Duration::from_secs(5),|
}

pub(super) async fn handle(
    cmd: CommandData<'_>,
    data: &ShardData,
    _args: Command,
) -> Result<(), CommandError> {
    let update_data = get_updates(data).await.map_err(|x| {
        tracing::error!("Error while getting update data: {:?}", x);
        let pretty_print = match x {
            UpdateRolesError::GuildRequest(_) => "Failed to gather guild data from Hypixel.",
            UpdateRolesError::Database(_) => "Failed to gather data from the database.",
            UpdateRolesError::GetGuildMembers(_) => "Failed to fetch guild members.",
            UpdateRolesError::InvalidDbUser(_) => "An user doesn't seem to have a valid linking status in the database. The state is corrupted.",
        };
        CommandError::Specific(String::from("There was a fatal error: ") + pretty_print)
    })?;

    if update_data.is_empty() {
        return cmd.reply(data, CommandResponseBuilder::new().etd("Update actions (dry run)", "No actions required.").build()).await;
    }

    let updates: Vec<String> = update_data.iter().map(|x| x.to_string()).collect();

    let mut responses = Vec::new();
    let mut replies = 0;
    let mut index = 0;
    loop {
        let (builder, new_index) = CommandResponseBuilder::new().split_across_embeds(make_descless_embed("Update actions (dry run)"), make_empty_embed(), "\n", &updates[index..])?;
        responses.push(builder.build());
        if new_index == 0 {
            break;
        }
        index += new_index;
        if replies >= 5 {
            responses.push(CommandResponseBuilder::new().ed("Not all actions have been displayed. They are too many.").build());
            break;
        }
        replies += 1;
    };

    let mut responses = responses.into_iter();

    cmd.reply(data, responses.next().unwrap()).await?;

    for response in responses {
        tokio::time::sleep(Duration::from_millis(500)).await;
        cmd.followup(data, response).await?;
    }

    Ok(())
}

pub(super) async fn check_gen_requirements(
    cmd: CommandData<'_>,
    _data: &ShardData,
) -> Result<(), MissingRequirement> {
    check_owner(cmd)
}

no_req!();
