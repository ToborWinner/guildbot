use twilight_model::channel::message::{component::{ActionRow, Button}, Component};

use super::{check_owner, CommandData, CommandError, CommandResponseBuilder, MissingRequirement};
use crate::ShardData;

make_command! {$
    [slash legacy]
    "sendverification"
    "Send the verification message."
    |cd: std::time::Duration::from_secs(3),|
}

pub(super) async fn handle(
    cmd: CommandData<'_>,
    data: &ShardData,
    _args: Command,
) -> Result<(), CommandError> {
    cmd.send(
        data,
        CommandResponseBuilder::new()
            .etd("DaySleepers | Hypixel Verification", "**Welcome to DaySleepers!**\n\nClick the button below and then insert your username to verify. If there are any issues, feel free to message one of the staff members.")
            .components(vec![Component::ActionRow(ActionRow {
                components: vec![Component::Button(Button {
                    custom_id: Some("verify".into()),
                    disabled: false,
                    emoji: None,
                    label: Some("Verify".into()),
                    style: twilight_model::channel::message::component::ButtonStyle::Primary,
                    url: None,
                    sku_id: None
                })]
            })])
            .build(),
    )
    .await?;

    cmd.reply(
        data,
        CommandResponseBuilder::new()
            .et("Verification message sent!")
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
