use super::{check_owner, CommandData, CommandError, CommandResponseBuilder, MissingRequirement};
use crate::ShardData;

make_command! {$
    [slash legacy]
    "updateloop"
    "Control the update loop"
    [Status, "status", "Get the update loop status."]:
    [Enable, "enable", "Enable the update loop."]:
    [Disable, "disable", "Disable the update loop."]:
}

pub(super) async fn handle(
    cmd: CommandData<'_>,
    data: &ShardData,
    args: Command,
) -> Result<(), CommandError> {
    cmd.reply(
        data,
        CommandResponseBuilder::new()
            .et(match args {
                Command::Status(Status) => format!{"Update loop enabled: `{}`", !data.paused_loop.load(std::sync::atomic::Ordering::SeqCst)},
                Command::Enable(Enable) => {
                    data.paused_loop.store(false, std::sync::atomic::Ordering::SeqCst);
                    "Update loop enabled!".to_owned()
                }
                Command::Disable(Disable) => {
                    data.paused_loop.store(true, std::sync::atomic::Ordering::SeqCst);
                    "Update loop disabled!".to_owned()
                }
            })
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
