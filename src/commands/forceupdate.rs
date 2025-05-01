use super::{check_owner, CommandData, CommandError, CommandResponseBuilder, MissingRequirement};
use crate::{minecraft::{send_notification, update_users}, ShardData};

make_command! {$
    [slash legacy]
    "forceupdate"
    "Forces an update"
    |cd: std::time::Duration::from_secs(30),|
}

pub(super) async fn handle(
    cmd: CommandData<'_>,
    data: &ShardData,
    _args: Command,
) -> Result<(), CommandError> {
    data.paused_loop.store(true, std::sync::atomic::Ordering::SeqCst);

    cmd.reply(
        data,
        CommandResponseBuilder::new()
            .etd("Starting forced update!", "**Note:** Update loop has been turned off. It will be turned back on after the update is complete.")
            .build(),
    ).await?;

    if let Err(e) = update_users(data).await {
        tracing::error!("Error while updating members: {}", e);
        send_notification(
            data,
            "there was an error in the update loop while updating members.",
        )
        .await;
    }

    data.paused_loop.store(false, std::sync::atomic::Ordering::SeqCst);

    Ok(())
}

pub(super) async fn check_gen_requirements(
    cmd: CommandData<'_>,
    _data: &ShardData,
) -> Result<(), MissingRequirement> {
    check_owner(cmd)
}

no_req!();
