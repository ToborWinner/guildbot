use link::link_user;
use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use twilight_http::{
    client::InteractionClient,
    request::{
        application::interaction::{CreateFollowup, UpdateResponse},
        channel::message::CreateMessage,
    },
};
use twilight_model::{
    application::{
        command::{Command, CommandType},
        interaction::{
            application_command::CommandDataOption,
            modal::{ModalInteractionDataActionRow, ModalInteractionDataComponent},
            InteractionData,
        },
    },
    channel::message::{
        component::{ActionRow, ComponentType, TextInput, TextInputStyle},
        AllowedMentions, Component, Embed, MessageFlags,
    },
    gateway::payload::incoming::{InteractionCreate, MessageCreate},
    http::{
        attachment::Attachment,
        interaction::{InteractionResponse, InteractionResponseData, InteractionResponseType},
    },
    id::{
        marker::{GuildMarker, UserMarker},
        Id,
    },
};
use twilight_util::builder::InteractionResponseDataBuilder;
use twilight_validate::embed::EmbedValidationError;

use crate::{
    database::{get_guild_prefix, DatabaseError, DEFAULT_PREFIX},
    ShardData, OWNERS,
};

macro_rules! arguments_struct {
    ($cmd:ident) => {
        #[derive(Debug)]
        pub(super) struct $cmd;
    };
    ($cmd:ident $(($rsname:ident, $name:literal, $desc:literal): <[$($typett:tt)+] $($attr:ident : $expr:expr;)*>)*) => {
        // TODO: Remove the dead_code attribute
        #[derive(Debug)]
        #[allow(dead_code)]
        pub(super) struct $cmd {
            $($rsname: make_type!($($typett)+),)*
        }
    };
}

macro_rules! is_required {
    ($type:ident ? $($tail:tt)*) => {
        0
    };
    ($type:ident = $($tail:tt)*) => {
        0
    };
    ($type:ident $($tail:tt)*) => {
        1
    };
}

macro_rules! plural_required {
    ([$typeid:ident $type:ident] $rsname:ident [$typeid2:ident $type2:ident] $rsname2:ident $([$typeido:ident $(?)? $(=$expr:expr;)? $typeo:ident] $rsnameo:ident)*) => {
        "s"
    };
    ($([$typeido:ident $(?)? $(=$expr:expr;)? $typeo:ident] $rsnameo:ident)*) => {
        ""
    };
}

macro_rules! handle_default_legacy {
    ($args:expr, $name:literal, $desc:literal [$typeid:ident ? $type:ty] $($attr:ident $expr:expr;)*) => {{
        match $args.next() {
            None => None,
            Some(arg) => Some(argument_parse_legacy![arg, $args, $name, $desc [$typeid $type] $($attr $expr;)*]),
        }
    }};
    ($args:expr, $name:literal, $desc:literal [$typeid:ident $type:ty] $($attr:ident $expr:expr;)*) => {{
        let arg = $args.next().unwrap();
        argument_parse_legacy![arg, $args, $name, $desc [$typeid $type] $($attr $expr;)*]
    }};
    ($args:expr, $name:literal, $desc:literal [$typeid:ident = $def:expr; $type:ty] $($attr:ident $expr:expr;)*) => {{
        match $args.next() {
            None => $def.to_owned(),
            Some(arg) => argument_parse_legacy![arg, $args, $name, $desc [$typeid $type] $($attr $expr;)*],
        }
    }};
}

macro_rules! handle_default_slash {
    ($args:expr, $name:literal, $desc:literal [$typeid:ident ? $type:ty] $($attr:ident $expr:expr;)*) => {{
        match $crate::commands::get_option($args, $name) {
            None => None,
            Some(twilight_model::application::interaction::application_command::CommandDataOption { value, .. }) => Some(argument_parse_slash![value, $name, $desc [$typeid $type] $($attr $expr;)*]),
        }
    }};
    ($args:expr, $name:literal, $desc:literal [$typeid:ident $type:ty] $($attr:ident $expr:expr;)*) => {{
        let twilight_model::application::interaction::application_command::CommandDataOption { value, .. } = $crate::commands::get_option($args, $name).unwrap();
        argument_parse_slash![value, $name, $desc [$typeid $type] $($attr $expr;)*]
    }};
    ($args:expr, $name:literal, $desc:literal [$typeid:ident = $def:expr; $type:ty] $($attr:ident $expr:expr;)*) => {{
        match $crate::commands::get_option($args, $name) {
            None => $def.to_owned(),
            Some(twilight_model::application::interaction::application_command::CommandDataOption { value, .. }) => argument_parse_slash![value, $name, $desc [$typeid $type] $($attr $expr;)*],
        }
    }};
}

// Begin type-specific macros

macro_rules! make_type {
    (int? $type:ty) => {
        Option<$type>
    };
    (int $(=$def:expr;)? $type:ty) => {
        $type
    };
    (float? $type:ty) => {
        Option<$type>
    };
    (float $(=$def:expr;)? $type:ty) => {
        $type
    };
    (word? $type:ty) => {
        Option<$type>
    };
    (word $(=$def:expr;)? $type:ty) => {
        $type
    };
    (string? $type:ty) => {
        Option<$type>
    };
    (string $(=$def:expr;)? $type:ty) => {
        $type
    };
    (user? $type:ty) => {
        Option<$type>
    };
    (user $(=$def:expr;)? $type:ty) => {
        $type
    };
}

macro_rules! argument_slash_builder {
    ($name:literal $desc:literal [int $type:ty] $($attr:ident $expr:expr;)*) => {{
        let builder = twilight_util::builder::command::IntegerBuilder::new($name, $desc);
        $(build_num_options![$attr builder $expr;];)*
        builder
    }};
    ($name:literal $desc:literal [float $type:ty] $($attr:ident $expr:expr;)*) => {{
        let builder = twilight_util::builder::command::NumberBuilder::new($name, $desc);
        $(build_num_options![$attr builder $expr;];)*
        builder
    }};
    ($name:literal $desc:literal [word $type:ty] $($attr:ident $expr:expr;)*) => {{
        let builder = twilight_util::builder::command::StringBuilder::new($name, $desc);
        $(build_string_options![$attr builder $expr;];)*
        builder
    }};
    ($name:literal $desc:literal [string $type:ty] $($attr:ident $expr:expr;)*) => {{
        let builder = twilight_util::builder::command::StringBuilder::new($name, $desc);
        $(build_string_options![$attr builder $expr;];)*
        builder
    }};
    ($name:literal $desc:literal [user $type:ty] $($attr:ident $expr:expr;)*) => {{
        twilight_util::builder::command::UserBuilder::new($name, $desc)
    }};
}

macro_rules! argument_parse_legacy {
    ($arg:ident, $args:expr, $name:literal, $desc:literal [int $(?)? $(=$def:expr;)? $type:ty] $($attr:ident $expr:expr;)*) => {{
        let arg = $arg.parse::<$type>().map_err(|_| {
            $crate::commands::CommandError::InvalidArgs(format!(
                concat!("The argument `", $name, "` must be an integer within specific constraints. You provided: `{}`"),
                $arg
            ))
        })?;
        $(handle_num_constraint![arg $name $attr $expr];)*
        arg
    }};
    ($arg:ident, $args:expr, $name:literal, $desc:literal [float $(?)? $(=$def:expr;)? $type:ty] $($attr:ident $expr:expr;)*) => {{
        let arg = $arg.parse::<$type>().map_err(|_| {
            $crate::commands::CommandError::InvalidArgs(format!(
                concat!("The argument `", $name, "` must be a number within specific constraints. You provided: `{}`"),
                $arg
            ))
        })?;
        $(handle_num_constraint![arg $name $attr $expr];)*
        arg
    }};
    ($arg:ident, $args:expr, $name:literal, $desc:literal [word $(?)? $(=$def:expr;)? $type:ty] $($attr:ident $expr:expr;)*) => {{
        let arg = $arg.to_owned();
        $(handle_string_constraint![arg $name $attr $expr];)*
        arg
    }};
    ($arg:ident, $args:expr, $name:literal, $desc:literal [string $(?)? $(=$def:expr;)? $type:ty] $($attr:ident $expr:expr;)*) => {{
        let mut last = None;
        while let Some(fin) = $args.next() {
            last = Some(fin);
        }
        let arg = match last {
            Some(fin) => {
                let start_ptr = $arg.as_ptr();
                let end_ptr = fin.as_ptr().add(fin.len());
                let len = end_ptr as usize - start_ptr as usize;
                std::str::from_utf8_unchecked(std::slice::from_raw_parts(start_ptr, len)).to_owned()
            },
            None => $arg.to_owned(),
        };
        $(handle_string_constraint![arg $name $attr $expr];)*
        arg
    }};
    ($arg:ident, $args:expr, $name:literal, $desc:literal [user $(?)? $(=$def:expr;)? $type:ty] $($attr:ident $expr:expr;)*) => {{
        let arg = $arg.strip_prefix("<@").unwrap_or($arg);
        let arg = arg.strip_prefix('!').unwrap_or(arg);
        let arg = arg.strip_suffix('>').unwrap_or($arg);
        let arg = arg.parse::<$type>().map_err(|_| {
            $crate::commands::CommandError::InvalidArgs(format!(
                concat!("The argument `", $name, "` must be a user's mention or id. You provided: `{}`"),
                $arg
            ))
        })?;
        arg
    }};
}

macro_rules! argument_parse_slash {
    ($value:ident, $name:literal, $desc:literal [int $(?)? $(=$def:expr;)? $type:ty] $($attr:ident $expr:expr;)*) => {
        if let twilight_model::application::interaction::application_command::CommandOptionValue::Integer(arg) = $value {
            let arg: $type = <i64 as TryInto<$type>>::try_into(*arg).map_err(|_| $crate::commands::CommandError::InvalidArgs(format!(
                concat!("The argument `", $name, "` must be an integer within specific constraints. You provided `{}`, which did not satify these constraints."), arg)))?;
            $(handle_num_constraint![arg $name $attr $expr];)*
            arg
        } else {
            tracing::error!(concat!("Invalid argument provided for slash command. Argument name: ", $name, ". Expected an integer, got: {:?}"), $value);
            return Err($crate::commands::CommandError::Generic);
        }
    };
    ($value:ident, $name:literal, $desc:literal [float $(?)? $(=$def:expr;)? $type:ty] $($attr:ident $expr:expr;)*) => {
        if let twilight_model::application::interaction::application_command::CommandOptionValue::Number(arg) = $value {
            let arg: $type = <f64 as TryInto<$type>>::try_into(*arg).map_err(|_| $crate::commands::CommandError::InvalidArgs(format!(
                concat!("The argument `", $name, "` must be a number within specific constraints. You provided `{}`, which did not satify these constraints."), arg)))?;
            $(handle_num_constraint![arg $name $attr $expr];)*
            arg
        } else {
            tracing::error!(concat!("Invalid argument provided for slash command. Argument name: ", $name, ". Expected a number, got: {:?}"), $value);
            return Err($crate::commands::CommandError::Generic);
        }
    };
    ($value:ident, $name:literal, $desc:literal [word $(?)? $(=$def:expr;)? $type:ty] $($attr:ident $expr:expr;)*) => {
        if let twilight_model::application::interaction::application_command::CommandOptionValue::String(arg) = $value {
            if arg.split_whitespace().nth(1).is_some() {
                return Err($crate::commands::CommandError::InvalidArgs(format!(
                    concat!("The argument `", $name, "` must be a single word. You provided: `{}`"), arg)));
            }
            let arg: $type = arg.to_owned();
            $(handle_string_constraint![arg $name $attr $expr];)*
            arg
        } else {
            tracing::error!(concat!("Invalid argument provided for slash command. Argument name: ", $name, ". Expected a word, got: {:?}"), $value);
            return Err($crate::commands::CommandError::Generic);
        }
    };
    ($value:ident, $name:literal, $desc:literal [string $(?)? $(=$def:expr;)? $type:ty] $($attr:ident $expr:expr;)*) => {
        if let twilight_model::application::interaction::application_command::CommandOptionValue::String(arg) = $value {
            let arg: $type = arg.to_owned();
            $(handle_string_constraint![arg $name $attr $expr];)*
            arg
        } else {
            tracing::error!(concat!("Invalid argument provided for slash command. Argument name: ", $name, ". Expected a string, got: {:?}"), $value);
            return Err($crate::commands::CommandError::Generic);
        }
    };
    ($value:ident, $name:literal, $desc:literal [user $(?)? $(=$def:expr;)? $type:ty] $($attr:ident $expr:expr;)*) => {
        if let twilight_model::application::interaction::application_command::CommandOptionValue::User(arg) = $value {
            <$type>::from(*arg)
        } else {
            tracing::error!(concat!("Invalid argument provided for slash command. Argument name: ", $name, ". Expected an user, got: {:?}"), $value);
            return Err($crate::commands::CommandError::Generic);
        }
    };
}

#[allow(unused_macros)]
macro_rules! build_num_options {
    (min $builder:ident $expr:expr;) => {
        let $builder = $builder.min_value($expr as _);
    };
    (max $builder:ident $expr:expr;) => {
        let $builder = $builder.max_value($expr as _);
    };
}

macro_rules! build_string_options {
    (min $builder:ident $expr:expr;) => {
        let $builder = $builder.min_length($expr as _);
    };
    (max $builder:ident $expr:expr;) => {
        let $builder = $builder.max_length($expr as _);
    };
}

#[allow(unused_macros)]
macro_rules! handle_num_constraint {
    ($arg:ident $name:literal max $expr:expr) => {
        if $arg > $expr {
            return Err($crate::commands::CommandError::InvalidArgs(format!(
                concat!(
                    "The argument `",
                    $name,
                    "` must be less than or equal to `{}`. You provided: `{}`"
                ),
                $expr, $arg
            )));
        }
    };
    ($arg:ident $name:literal min $expr:expr) => {
        if $arg < $expr {
            return Err($crate::commands::CommandError::InvalidArgs(format!(
                concat!(
                    "The argument `",
                    $name,
                    "` must be greater than or equal to `{}`. You provided: `{}`"
                ),
                $expr, $arg
            )));
        }
    };
}

macro_rules! handle_string_constraint {
    ($arg:ident $name:literal max $expr:expr) => {
        if $arg.chars().count() > $expr {
            return Err($crate::commands::CommandError::InvalidArgs(format!(
                concat!(
                    "The argument `",
                    $name,
                    "` must have a length smaller than or equal to `{}`. You provided: `{}`, which is too long."
                ),
                $expr, $arg
            )));
        }
    };
    ($arg:ident $name:literal min $expr:expr) => {
        if $arg.chars().count() < $expr {
            return Err($crate::commands::CommandError::InvalidArgs(format!(
                concat!(
                    "The argument `",
                    $name,
                    "` must have a length greater than or equal to `{}`. You provided: `{}`, which is too short."
                ),
                $expr, $arg
            )));
        }
    };
}

// End type-specific macros

macro_rules! arguments_parse_legacy {
    ($args:ident $cmd:ident) => {
        {
            if $args.len() > 0 {
                return Err($crate::commands::CommandError::InvalidArgs(format!("You provided too many arguments for this command. Expected a maximum of `0` arguments, but you provided `{}`.", $args.len()).to_owned()));
            }

            Ok::<_, $crate::commands::CommandError>($cmd)
        }
    };
    ($args:ident $cmd:ident $(($rsname:ident, $name:literal, $desc:literal): <[$($typett:tt)+] $($attr:ident : $expr:expr;)*>)*) => {
        {
            const AMOUNT_REQUIRED: usize = 0 $(+ is_required!($($typett)+ $rsname))*;
            if $args.len() < AMOUNT_REQUIRED {
                return Err($crate::commands::CommandError::InvalidArgs(format!(concat!("This command has `{}` required argument", plural_required!($([$($typett)+] $rsname)*),", but you only provided `{}`."), AMOUNT_REQUIRED, $args.len())));
            }

            let res = $cmd {
                $($rsname: handle_default_legacy![$args, $name, $desc [$($typett)+] $($attr $expr;)*],)*
            };

            if $args.len() > 0 {
                const AMOUNT_TOTAL: usize = count_ident!($($rsname)*);
                return Err($crate::commands::CommandError::InvalidArgs(format!("You provided too many arguments for this command. Expected a maximum of `{}` arguments, but you provided `{}`.", AMOUNT_TOTAL, AMOUNT_TOTAL + $args.len()).to_owned()));
            }

            Ok::<_, $crate::commands::CommandError>(res)
        }
    };
}

macro_rules! arguments_parse_slash {
    ($args:ident $cmd:ident) => {
        {
            Ok::<_, $crate::commands::CommandError>($cmd)
        }
    };
    ($args:ident $cmd:ident $(($rsname:ident, $name:literal, $desc:literal): <[$($typett:tt)+] $($attr:ident : $expr:expr;)*>)*) => {
        {
            Ok::<_, $crate::commands::CommandError>($cmd {
                $($rsname: handle_default_slash![$args, $name, $desc [$($typett)+] $($attr $expr;)*],)*
            })
        }
    };
}

macro_rules! subcommands_enum {
    ($cmd:ident $([$subcmd:ident, $subname:literal, $subdesc:literal]: $(($rsname:ident, $name:literal, $desc:literal): <[$($typett:tt)+] $($attr:ident : $expr:expr;)*>)*)+) => {
        // TODO: Remove the dead_code attribute
        #[derive(Debug)]
        #[allow(dead_code)]
        pub(super) enum $cmd {
            $($subcmd ($subcmd),)+
        }
        $(arguments_struct![$subcmd $(($rsname, $name, $desc): <[$($typett)+] $($attr : $expr;)*>)*];)+
    };
}

#[allow(unused_macros)]
macro_rules! subcommand_groups_enum {
    ($cmd:ident $({$subgroupcmd:ident, $subgroupname:literal, $subgroupdesc:literal}: $([$subcmd:ident, $subname:literal, $subdesc:literal]: $(($rsname:ident, $name:literal, $desc:literal): <[$($typett:tt)+] $($attr:ident : $expr:expr;)*>)*)+)+) => {
        // TODO: Remove the dead_code attribute
        #[derive(Debug)]
        #[allow(dead_code)]
        pub(super) enum $cmd {
            $($subgroupcmd ($subgroupcmd),)+
        }
        $(subcommands_enum![$subgroupcmd $([$subcmd, $subname, $subdesc]: $(($rsname, $name, $desc): <[$($typett)+] $($attr : $expr;)*>)*)+];)+
    };
}

macro_rules! separate_literals_comma {
    ($first:literal $($rest:literal)*) => {
        concat![$first $(, ", ", $rest)*]
    };
}

macro_rules! subcommands_parse_legacy {
    ($args:ident $cmd:ident $([$subcmd:ident, $subname:literal, $subdesc:literal]: $(($rsname:ident, $name:literal, $desc:literal): <[$($typett:tt)+] $($attr:ident : $expr:expr;)*>)*)+) => {
        {
            Ok::<_, $crate::commands::CommandError>(match $args.next().ok_or($crate::commands::CommandError::InvalidArgs(concat!("You must specify a subcommand to execute.\n> Available subcommands: `", separate_literals_comma![$($subname)+], "`").to_owned()))? {
                $($subname => $cmd::$subcmd (arguments_parse_legacy![$args $subcmd $(($rsname, $name, $desc): <[$($typett)+] $($attr : $expr;)*>)*]?),)+
                _ => return Err($crate::commands::CommandError::InvalidArgs(concat!("The subcommand you provided does not exist.\n> Available subcommands: `", separate_literals_comma![$($subname)+], "`").to_owned())),
            })
        }
    };
}

macro_rules! subcommands_parse_slash {
    ($args:ident $cmd:ident $([$subcmd:ident, $subname:literal, $subdesc:literal]: $(($rsname:ident, $name:literal, $desc:literal): <[$($typett:tt)+] $($attr:ident : $expr:expr;)*>)*)+) => {
        {
            if let Some(twilight_model::application::interaction::application_command::CommandDataOption { ref name, value: twilight_model::application::interaction::application_command::CommandOptionValue::SubCommand(ref arg)}) = $args.into_iter().next() {
                match name.as_str() {
                    $($subname => Ok($cmd::$subcmd (arguments_parse_slash![arg $subcmd $(($rsname, $name, $desc): <[$($typett)+] $($attr : $expr;)*>)*]?)),)+
                    _ => {
                        tracing::error!("Expected a valid subcommand in slash command, got something else.");
                        Err($crate::commands::CommandError::InvalidArgs(concat!("The subcommand you provided does not exist.\n> Available subcommands: `", separate_literals_comma![$($subname)+], "`").to_owned()))
                    }
                }
            } else {
                tracing::error!("Expected a subcommand in slash command, got something else.");
                Err($crate::commands::CommandError::Generic)
            }
        }
    };
}

#[allow(unused_macros)]
macro_rules! subcommand_groups_parse_legacy {
    ($args:ident $cmd:ident $({$subgroupcmd:ident, $subgroupname:literal, $subgroupdesc:literal}: $([$subcmd:ident, $subname:literal, $subdesc:literal]: $(($rsname:ident, $name:literal, $desc:literal): <[$($typett:tt)+] $($attr:ident : $expr:expr;)*>)*)+)+) => {
        {
            Ok(match $args.next().ok_or($crate::commands::CommandError::InvalidArgs(concat!("You must specify a subcommand group to execute.\n> Available subcommand groups: `", separate_literals_comma![$($subgroupname)+], "`").to_owned()))? {
                $($subgroupname => $cmd::$subgroupcmd (subcommands_parse_legacy![$args $subgroupcmd $([$subcmd, $subname, $subdesc]: $(($rsname, $name, $desc): <[$($typett)+] $($attr : $expr;)*>)*)+]?),)+
                _ => return Err($crate::commands::CommandError::InvalidArgs(concat!("The subcommand group you provided does not exist.\n> Available subcommand groups: `", separate_literals_comma![$($subgroupname)+], "`").to_owned())),
            })
        }
    };
}

#[allow(unused_macros)]
macro_rules! subcommand_groups_parse_slash {
    ($args:ident $cmd:ident $({$subgroupcmd:ident, $subgroupname:literal, $subgroupdesc:literal}: $([$subcmd:ident, $subname:literal, $subdesc:literal]: $(($rsname:ident, $name:literal, $desc:literal): <[$($typett:tt)+] $($attr:ident : $expr:expr;)*>)*)+)+) => {
        {
            if let Some(twilight_model::application::interaction::application_command::CommandDataOption { ref name, value: twilight_model::application::interaction::application_command::CommandOptionValue::SubCommandGroup(ref arg)}) = $args.into_iter().next() {
                match name.as_str() {
                    $($subgroupname => Ok($cmd::$subgroupcmd (subcommands_parse_slash![arg $subgroupcmd $([$subcmd, $subname, $subdesc]: $(($rsname, $name, $desc): <[$($typett)+] $($attr : $expr;)*>)*)+]?)),)+
                    _ => {
                        tracing::error!("Expected a valid subcommand group in slash command, got something else.");
                        Err($crate::commands::CommandError::InvalidArgs(concat!("The subcommand group you provided does not exist.\n> Available subcommands: `", separate_literals_comma![$($subgroupname)+], "`").to_owned()))
                    }
                }
            } else {
                tracing::error!("Expected a subcommand group in slash command, got something else.");
                Err($crate::commands::CommandError::Generic)
            }
        }
    };
}

macro_rules! argument_slash_req_builder {
    ($name:literal $desc:literal [$typeid:ident ? $type:ty] $($attr:ident $expr:expr;)*) => {{
        argument_slash_builder!($name $desc [$typeid $type] $($attr $expr;)*).required(false)
    }};
    ($name:literal $desc:literal [$typeid:ident $type:ty] $($attr:ident $expr:expr;)*) => {{
        argument_slash_builder!($name $desc [$typeid $type] $($attr $expr;)*).required(true)
    }};
    ($name:literal $desc:literal [$typeid:ident = $def:expr; $type:ty] $($attr:ident $expr:expr;)*) => {{
        argument_slash_builder!($name $desc [$typeid $type] $($attr $expr;)*).required(false)
    }};
}

// TODO: Test removing the {} scope
macro_rules! arguments_slash_builder {
    ($builder:expr; $(($rsname:ident, $name:literal, $desc:literal): <[$($typett:tt)+] $($attr:ident : $expr:expr;)*>)*) => { {
        $builder $(.option(argument_slash_req_builder!($name $desc [$($typett)+] $($attr $expr;)*)))*
    }}
}

#[allow(unused_macros)]
macro_rules! arguments_slash_subcommand_builder {
    ($builder:expr; $([$subcmd:ident, $subname:literal, $subdesc:literal]: $(($rsname:ident, $name:literal, $desc:literal): <[$($typett:tt)+] $($attr:ident : $expr:expr;)*>)*)+) => {
        $builder $(.option(arguments_slash_builder![twilight_util::builder::command::SubCommandBuilder::new($subname, $subdesc); $(($rsname, $name, $desc): <[$($typett)+] $($attr : $expr;)*>)*]))+
    }
}

#[allow(unused_macros)]
macro_rules! arguments_slash_subcommand_group_builder {
    ($builder:expr; $({$subgroupcmd:ident, $subgroupname:literal, $subgroupdesc:literal}: $([$subcmd:ident, $subname:literal, $subdesc:literal]: $(($rsname:ident, $name:literal, $desc:literal): <[$($typett:tt)+] $($attr:ident : $expr:expr;)*>)*)+)+) => {
        $builder $(.option(twilight_util::builder::command::SubCommandGroupBuilder::new($subgroupname, $subgroupdesc).subcommands([$(arguments_slash_builder![twilight_util::builder::command::SubCommandBuilder::new($subname, $subdesc); $(($rsname, $name, $desc): <[$($typett)+] $($attr : $expr;)*>)*]),+])))+
    }
}

macro_rules! handle_default_usage_short {
    ($name:literal [$typeid:ident ? $type:ty]) => {
        concat!["[", $name, "]"]
    };
    ($name:literal [$typeid:ident $type:ty]) => {
        concat!["<", $name, ">"]
    };
    ($name:literal [$typeid:ident = $def:expr; $type:ty]) => {
        concat!["[", $name, "]"]
    };
}

macro_rules! handle_default_usage {
    ($space:expr; $name:literal $desc:literal [$typeid:ident ? $type:ty] $($attr:ident $expr:expr;)*) => {
        concat!["[", $name $(, " ", stringify![$attr], ": ", stringify![$expr])*, "]\n", $space, "  ", $desc]
    };
    ($space:expr; $name:literal $desc:literal [$typeid:ident $type:ty] $($attr:ident $expr:expr;)*) => {
        concat!["<", $name $(, " ", stringify![$attr], ": ", stringify![$expr])*, ">\n", $space, "  ", $desc]
    };
    ($space:expr; $name:literal $desc:literal [$typeid:ident = $def:expr; $type:ty] $($attr:ident $expr:expr;)*) => {
        concat!["[", $name $(, " ", stringify![$attr], ": ", stringify![$expr])*, " default: ", stringify![$def], "]\n", $space, "  ", $desc]
    };
}

macro_rules! arguments_usage {
    ($space:expr; $curr:expr;) => {
        concat![$space, "Syntax: ", $curr]
    };
    ($space:expr; $curr:expr; $(($rsname:ident, $name:literal, $desc:literal): <[$($typett:tt)+] $($attr:ident : $expr:expr;)*>)*) => {
        concat![$space, "Syntax: ", $curr $(, " ", handle_default_usage_short![$name [$($typett)+]])*, "\n", $space, "Details:" $(, "\n", $space, "- ", handle_default_usage![$space; $name $desc [$($typett)+] $($attr $expr;)*])*]
    };
}

macro_rules! subcommand_arguments_usage {
    ($space:expr; $curr:expr; $([$subcmd:ident, $subname:literal, $subdesc:literal]: $(($rsname:ident, $name:literal, $desc:literal): <[$($typett:tt)+] $($attr:ident : $expr:expr;)*>)*)+) => {
        concat![$space, "Syntax: ", $curr, " <subcommand> [arguments]\n", $space, "Subcommands:" $(, "\n", $space, "  - ", $subname, " => ", $subdesc, "\n", arguments_usage![concat![$space, "    "]; concat![$curr, " ", $subname]; $(($rsname, $name, $desc): <[$($typett)+] $($attr : $expr;)*>)*], "\n")+]
    };
}

#[allow(unused_macros)]
macro_rules! subcommand_group_arguments_usage {
    ($space:expr; $curr:expr; $({$subgroupcmd:ident, $subgroupname:literal, $subgroupdesc:literal}: $([$subcmd:ident, $subname:literal, $subdesc:literal]: $(($rsname:ident, $name:literal, $desc:literal): <[$($typett:tt)+] $($attr:ident : $expr:expr;)*>)*)+)+) => {
        concat![$space, "Syntax: ", $curr, " <subcommand-group> <subcommand> [arguments]\n\n", $space, "Subcommand Groups:" $(, "\n", $space, "* ", $subgroupname, " => ", $subgroupdesc, "\n", subcommand_arguments_usage![concat!($space, "  "); concat!($curr, " ", $subgroupname); $([$subcmd, $subname, $subdesc]: $(($rsname, $name, $desc): <[$($typett)+] $($attr : $expr;)*>)*)+], "\n")+]
    };
}

macro_rules! usage_wrapper {
    ($name:literal $desc:literal $macro:ident $($args:tt)*) => {
        concat![$name, " => ", $desc, "\n", $macro!(""; $name; $($args)*)]
    }
}

macro_rules! parse_signature_legacy {
    ($args:ident $legacy:block) => {
        pub(super) unsafe fn parse_arguments_legacy(
            $args: Vec<&str>,
        ) -> Result<Command, $crate::commands::CommandError> {
            #[allow(unused_mut)]
            #[allow(unused_variables)]
            let mut $args = $args.into_iter();
            $legacy
        }
    };
}

macro_rules! parse_signature_slash {
    ($args:ident $slash:block $builder:block) => {
        #[allow(unused_variables)]
        pub(super) fn parse_arguments_slash(
            $args: &[twilight_model::application::interaction::application_command::CommandDataOption],
        ) -> Result<Command, $crate::commands::CommandError> {
            $slash
        }

        pub(super) fn build_command() -> twilight_util::builder::command::CommandBuilder {
            $builder
        }
    };
}

macro_rules! parse_signature_common {
    ($usage:expr) => {
        pub(super) const USAGE: &'static str = $usage;
    };
}

macro_rules! make_specific {
    (slash $name:literal $desc:literal $(|$($iname:ident : $iexpr:expr ,)*|)?) => {
        parse_signature_slash!(args {
            arguments_parse_slash! {
                args
                Command
            }
        }
        {
            arguments_slash_builder! {
                twilight_util::builder::command::CommandBuilder::new($name, $desc, twilight_model::application::command::CommandType::ChatInput);
            }
        });
    };
    (slash $name:literal $desc:literal $(|$($iname:ident : $iexpr:expr ,)*|)? ($($args:tt)*) $($tail:tt)*) => {
        parse_signature_slash!(args {
            arguments_parse_slash! {
                args
                Command ($($args)*) $($tail)*
            }
        }
        {
            arguments_slash_builder! {
                twilight_util::builder::command::CommandBuilder::new($name, $desc, twilight_model::application::command::CommandType::ChatInput);
                ($($args)*) $($tail)*
            }
        });
    };
    (slash $name:literal $desc:literal $(|$($iname:ident : $iexpr:expr ,)*|)? [$($args:tt)*] $($tail:tt)*) => {
        parse_signature_slash!(args {
            subcommands_parse_slash! {
                args
                Command [$($args)*] $($tail)*
            }
        }
        {
            arguments_slash_subcommand_builder! {
                twilight_util::builder::command::CommandBuilder::new($name, $desc, twilight_model::application::command::CommandType::ChatInput);
                [$($args)*] $($tail)*
            }
        });
    };
    (slash $name:literal $desc:literal $(|$($iname:ident : $iexpr:expr ,)*|)? {$($args:tt)*} $($tail:tt)*) => {
        parse_signature_slash!(args {
            subcommand_groups_parse_slash! {
                args
                Command {$($args)*} $($tail)*
            }
        }
        {
            arguments_slash_subcommand_group_builder! {
                twilight_util::builder::command::CommandBuilder::new($name, $desc, twilight_model::application::command::CommandType::ChatInput);
                {$($args)*} $($tail)*
            }
        });
    };
    (legacy $name:literal $desc:literal $(|$($iname:ident : $iexpr:expr ,)*|)?) => {
        parse_signature_legacy!(args {
            arguments_parse_legacy! {
                args
                Command
            }
        });
    };
    (legacy $name:literal $desc:literal $(|$($iname:ident : $iexpr:expr ,)*|)? ($($args:tt)*) $($tail:tt)*) => {
        parse_signature_legacy!(args {
            arguments_parse_legacy! {
                args
                Command ($($args)*) $($tail)*
            }
        });
    };
    (legacy $name:literal $desc:literal $(|$($iname:ident : $iexpr:expr ,)*|)? [$($args:tt)*] $($tail:tt)*) => {
        parse_signature_legacy!(args {
            subcommands_parse_legacy! {
                args
                Command [$($args)*] $($tail)*
            }
        });
    };
    (legacy $name:literal $desc:literal $(|$($iname:ident : $iexpr:expr ,)*|)? {$($args:tt)*} $($tail:tt)*) => {
        parse_signature_legacy!(args {
            subcommand_groups_parse_legacy! {
                args
                Command {$($args)*} $($tail)*
            }
        });
    };
}

macro_rules! make_cooldown {
    () => {
        pub(super) const COOLDOWN: std::time::Duration = std::time::Duration::from_secs(1);
    };
    (cd : $dur:expr , $($args:tt)*) => {
        pub(super) const COOLDOWN: std::time::Duration = $dur;
    };
    ($first:ident : $val:expr , $($args:tt)*) => {
        make_cooldown![$($args)*];
    };
}

macro_rules! make_command_common {
    ($name:literal $desc:literal $(|$($iname:ident : $iexpr:expr ,)*|)?) => {
        arguments_struct!(Command);
        parse_signature_common!(usage_wrapper![$name $desc arguments_usage]);
        make_cooldown![$($($iname : $iexpr ,)*)?];
    };
    ($name:literal $desc:literal $(|$($iname:ident : $iexpr:expr ,)*|)? ($($args:tt)*) $($tail:tt)*) => {
        arguments_struct!(Command ($($args)*) $($tail)*);
        parse_signature_common!(usage_wrapper![$name $desc arguments_usage ($($args)*) $($tail)*]);
        make_cooldown![$($($iname : $iexpr ,)*)?];
    };
    ($name:literal $desc:literal $(|$($iname:ident : $iexpr:expr ,)*|)? [$($args:tt)*] $($tail:tt)*) => {
        subcommands_enum!(Command [$($args)*] $($tail)*);
        parse_signature_common!(usage_wrapper![$name $desc subcommand_arguments_usage [$($args)*] $($tail)*]);
        make_cooldown![$($($iname : $iexpr ,)*)?];
    };
    ($name:literal $desc:literal $(|$($iname:ident : $iexpr:expr ,)*|)? {$($args:tt)*} $($tail:tt)*) => {
        subcommand_groups_enum!(Command {$($args)*} $($tail)*);
        parse_signature_common!(usage_wrapper![$name $desc subcommand_group_arguments_usage {$($args)*} $($tail)*]);
        make_cooldown![$($($iname : $iexpr ,)*)?];
    };
}

macro_rules! make_command_sub {
    ([$first:ident $($type:ident)*] $($tail:tt)*) => {
        make_specific![$first $($tail)*];
        make_command_sub!([$($type)*] $($tail)*);
    };
    ([] $($tail:tt)*) => {};
}

macro_rules! make_match_statement {
    (make_slash $($name:ident)*) => {
        #[inline]
        async fn exec_chatinput_command(event: &InteractionCreate, data: &ShardData, keyword: &str, args: &[CommandDataOption]) {
            let cmd = CommandData::ChatInput(event);
            match keyword {
                $(stringify!($name) => unsafe { handle_command(cmd, data, args, &data.command_cooldowns.$name, event.author_id().unwrap(), $name::USAGE, $name::parse_arguments_slash, $name::check_requirements, $name::check_gen_requirements, $name::handle).await },)*
                _ => (),
            }
        }

        #[inline]
        fn get_commands() -> Result<[Command; count_ident!($($name)*)], Box<dyn std::error::Error + Send + Sync>> {
            Ok([
                $($name::build_command().validate()?.build(),)*
            ])
        }
    };
    (make_legacy $keyword:ident $cmd:ident $data:ident $args:ident $event:ident $($name:ident)*) => {
        match $keyword {
            $(stringify!($name) => handle_command($cmd, $data, $args, &$data.command_cooldowns.$name, $event.author.id, $name::USAGE, $name::parse_arguments_legacy, $name::check_requirements, $name::check_gen_requirements, $name::handle).await,)*
            _ => (),
        }
    };
}

macro_rules! make_macro {
    ($dollar:tt $name:ident yes) => {
        macro_rules! $name {
            ($dollar me:ident $dollar first:ident $dollar ($dollar cmd:ident)* [$dollar ($dollar yes:ident)*] $dollar ($dollar args:tt)*) => {
                $dollar first::$name!{$dollar first $dollar ($dollar cmd)* [$dollar me $dollar ($dollar yes)*] $dollar ($dollar args)*}
            };
            ($dollar me:ident [$dollar ($dollar yes:ident)*] $dollar ($dollar args:tt)*) => {
                make_match_statement![$name $dollar ($dollar args)* $dollar me $dollar ($dollar yes)*];
            };
        }
        pub(super) use $name;
    };
    ($dollar:tt $name:ident no) => {
        macro_rules! $name {
            ($dollar me:ident $dollar first:ident $dollar ($dollar cmd:ident)* [$dollar ($dollar yes:ident)*] $dollar ($dollar args:tt)*) => {
                $dollar first::$name!{$dollar first $dollar ($dollar cmd)* [$dollar ($dollar yes)*] $dollar ($dollar args)*}
            };
            ($dollar me:ident [$dollar ($dollar yes:ident)*] $dollar ($dollar args:tt)*) => {
                make_match_statement![$name $dollar ($dollar args)* $dollar ($dollar yes)*]
            };
        }
        pub(super) use $name;
    };
}

macro_rules! make_macros {
    ($dollar:tt slash legacy) => {
        make_macro![$dollar make_slash yes];
        make_macro![$dollar make_legacy yes];
    };
    ($dollar:tt legacy slash) => {
        make_macro![$dollar make_slash yes];
        make_macro![$dollar make_legacy yes];
    };
    ($dollar:tt slash) => {
        make_macro![$dollar make_slash yes];
        make_macro![$dollar make_legacy no];
    };
    ($dollar:tt legacy) => {
        make_macro![$dollar make_slash no];
        make_macro![$dollar make_legacy yes];
    };
}

macro_rules! make_command {
    ($dollar:tt [$($ident:tt)*] $($tt:tt)*) => {
        make_command_common![$($tt)*];
        make_command_sub!([$($ident)*] $($tt)*);
        make_macros!($dollar $($ident)*);
    };
}

macro_rules! count_ident {
    () => { 0 };
    ($odd:ident $($a:ident $b:ident)*) => { (count_ident!($($a)*) << 1) | 1 };
    ($($a:ident $even:ident)*) => { count_ident!($($a)*) << 1 };
}

macro_rules! include_commands {
    ($first:ident $($name:ident)*) => {
        pub(super) mod $first;
        $(pub(super) mod $name;)*

        #[inline]
        async unsafe fn exec_legacy_command(event: &MessageCreate, data: &ShardData, keyword: &str, args: Vec<&str>) {
            let cmd = CommandData::Legacy(event);
            $first::make_legacy![$first $($name)* [] keyword cmd data args event];
        }

        $first::make_slash!($first $($name)* []);

        #[derive(Debug)]
        pub struct CommandCooldownList {
            $first: std::sync::Mutex<CommandCooldown>,
            $($name: std::sync::Mutex<CommandCooldown>,)*
        }

        impl CommandCooldownList {
            pub fn new() -> Self {
                Self {
                    $first: std::sync::Mutex::new(CommandCooldown::new($first::COOLDOWN)),
                    $($name: std::sync::Mutex::new(CommandCooldown::new($name::COOLDOWN)),)*
                }
            }
        }
    };
}

impl Default for CommandCooldownList {
    fn default() -> Self {
        Self::new()
    }
}

/// Signifies what type of error the command returned. This will be used to determine what action
/// to take, such as replying to the user command with a generic error message or with a specific
/// error message.
// TODO: Remove the dead_code attribute
#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
enum CommandError {
    /// `NoMsgPermission` signifies that the bot does not have the permission to reply to the user
    /// and there should therefore be no further attempts.
    #[error("The bot does not have the necessary permissions to reply")]
    NoMsgPermission,

    /// `MissingRequirement` signifies that the user does not have the necessary permissions to
    /// run the command.
    #[error("Missing requirement: {0}")]
    MissingRequirement(#[from] MissingRequirement),

    /// Reply with an error message that the user has provided invalid arguments.
    #[error("Invalid arguments provided: {0}")]
    InvalidArgs(String),

    /// Reply with an error message that the user has provided invalid arguments, along with the
    /// usage of the command.
    #[error("Invalid arguments provided: {0}. Usage: {1}")]
    InvalidArgsWithUsage(String, &'static str),

    /// Reply with an error message that the command is on cooldown for the provided duration.
    #[error("The command is on cooldown. Time left on cooldown: {0:?}")]
    Cooldown(Duration),

    /// Reply with a generic error message.
    #[error("Unexpected error")]
    Generic,

    /// Reply with a neatly formatted message containig the specific error string.
    #[error("Specific error: {0}")]
    Specific(String),
}

impl From<DatabaseError> for CommandError {
    fn from(_: DatabaseError) -> Self {
        Self::Generic
    }
}

impl From<EmbedValidationError> for CommandError {
    fn from(e: EmbedValidationError) -> Self {
        tracing::error!("Error when validating embed: {}", e);
        Self::Generic
    }
}

impl From<SplitAcrossEmbedsError> for CommandError {
    fn from(e: SplitAcrossEmbedsError) -> Self {
        tracing::error!("Error when splitting embed: {}", e);
        match e {
            SplitAcrossEmbedsError::Unsplittable => Self::Generic,
        }
    }
}

// TODO: Remove the dead_code attribute
#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
enum MissingRequirement {
    #[error("The user does not have the necessary permissions to run this command: It is bot owner only.")]
    BotOwnerOnly,
    #[error("Missing permissions: {0}")]
    General(String),
}

trait AsyncFn3<Arg0, Arg1, Arg2>: Fn(Arg0, Arg1, Arg2) -> Self::OutputFuture {
    type OutputFuture: Future<Output = <Self as AsyncFn3<Arg0, Arg1, Arg2>>::Output>;
    type Output;
}

impl<F, Fut, Arg0, Arg1, Arg2> AsyncFn3<Arg0, Arg1, Arg2> for F
where
    F: Fn(Arg0, Arg1, Arg2) -> Fut + ?Sized,
    Fut: Future,
{
    type OutputFuture = Fut;
    type Output = Fut::Output;
}

#[inline]
#[allow(clippy::too_many_arguments)]
async unsafe fn handle_command<'a, FutHandle, FutGenReq, ArgsInput, Args>(
    cmd: CommandData<'a>,
    data: &'a ShardData,
    args: ArgsInput,
    cd: &'a Mutex<CommandCooldown>,
    author: Id<UserMarker>,
    usage: &'static str,
    parse: unsafe fn(ArgsInput) -> Result<Args, CommandError>,
    req: impl for<'b> AsyncFn3<
        CommandData<'a>,
        &'a ShardData,
        &'b Args,
        Output = Result<(), MissingRequirement>,
    >,
    req_gen: impl Fn(CommandData<'a>, &'a ShardData) -> FutGenReq,
    handle: impl Fn(CommandData<'a>, &'a ShardData, Args) -> FutHandle,
) where
    FutHandle: Future<Output = Result<(), CommandError>>,
    FutGenReq: Future<Output = Result<(), MissingRequirement>>,
{
    let cd_check = cd.lock().unwrap().check(author);
    if let Some(duration) = cd_check {
        let _ = cmd.reply(data, CommandError::Cooldown(duration)).await;
        return;
    }

    match match req_gen(cmd, data).await {
        Ok(_) => match parse(args) {
            Ok(args) => match req(cmd, data, &args).await {
                Ok(_) => handle(cmd, data, args).await,
                Err(missing) => Err(missing.into()),
            },
            Err(CommandError::InvalidArgs(msg)) => {
                Err(CommandError::InvalidArgsWithUsage(msg, usage))
            }
            Err(e) => Err(e),
        },
        Err(missing) => Err(missing.into()),
    } {
        Ok(_) => (),
        Err(CommandError::NoMsgPermission) => (),
        Err(CommandError::MissingRequirement(MissingRequirement::BotOwnerOnly))
            if matches!(cmd, CommandData::Legacy(_)) => {}
        Err(e) => {
            let _ = cmd.reply(data, e).await;
        }
    }
}

#[inline]
fn get_option<'a>(list: &'a [CommandDataOption], name: &str) -> Option<&'a CommandDataOption> {
    list.iter().find(|opt| opt.name == name)
}

#[derive(Copy, Clone, Debug)]
enum CommandData<'a> {
    Legacy(&'a MessageCreate),
    ChatInput(&'a InteractionCreate),
}

impl CommandData<'_> {
    async fn reply(
        &self,
        data: &ShardData,
        response: impl Into<CommandResponse>,
    ) -> Result<(), CommandError> {
        match self {
            Self::Legacy(event) => {
                let _ = response
                    .into()
                    .add_to_create_message(data.client.create_message(event.channel_id))
                    .reply(event.id)
                    .await
                    .map_err(|_| CommandError::NoMsgPermission)?;
            }
            Self::ChatInput(event) => {
                data.client
                    .interaction(event.application_id)
                    .create_response(
                        event.id,
                        &event.token,
                        &InteractionResponse {
                            kind: InteractionResponseType::ChannelMessageWithSource,
                            data: Some(response.into().into()),
                        },
                    )
                    .await
                    .map_err(|_| CommandError::Generic)?;
            }
        };

        Ok(())
    }

    async fn send(
        &self,
        data: &ShardData,
        response: impl Into<CommandResponse>,
    ) -> Result<(), CommandError> {
        let channel_id = match self {
            Self::Legacy(event) => event.channel_id,
            Self::ChatInput(event) => event.channel.as_ref().ok_or(CommandError::Generic)?.id,
        };

        let _ = response
            .into()
            .add_to_create_message(data.client.create_message(channel_id))
            .await
            .map_err(|_| CommandError::NoMsgPermission)?;

        Ok(())
    }

    async fn followup(
        &self,
        data: &ShardData,
        response: impl Into<CommandResponse>,
    ) -> Result<(), CommandError> {
        match self {
            Self::Legacy(event) => {
                let _ = response
                    .into()
                    .add_to_create_message(data.client.create_message(event.channel_id))
                    .reply(event.id)
                    .await
                    .map_err(|_| CommandError::NoMsgPermission)?;
            }
            Self::ChatInput(event) => {
                let _ = response
                    .into()
                    .add_to_create_followup(
                        data.client
                            .interaction(event.application_id)
                            .create_followup(&event.token),
                    )
                    .await
                    .map_err(|_| CommandError::Generic)?;
            }
        };

        Ok(())
    }

    pub fn guild_id(&self) -> Option<Id<GuildMarker>> {
        match self {
            Self::Legacy(event) => event.guild_id,
            Self::ChatInput(event) => event.guild_id,
        }
    }

    pub fn author_id(&self) -> Option<Id<UserMarker>> {
        match self {
            Self::Legacy(event) => Some(event.author.id),
            Self::ChatInput(event) => event.author_id(),
        }
    }
}

#[derive(Debug, Default)]
struct CommandResponse {
    content: Option<String>,
    allowed_mentions: Option<AllowedMentions>,
    attachments: Option<Vec<Attachment>>,
    components: Option<Vec<Component>>,
    embeds: Option<Vec<Embed>>,
    flags: Option<MessageFlags>,
    // sticker_ids is also possible for messages, but not interactions
}

impl CommandResponse {
    fn add_to_create_message<'a>(
        &'a self,
        mut create_message: CreateMessage<'a>,
    ) -> CreateMessage<'a> {
        if let Some(content) = &self.content {
            create_message = create_message.content(content);
        }
        if let Some(allowed_mentions) = &self.allowed_mentions {
            create_message = create_message.allowed_mentions(Some(allowed_mentions));
        }
        if let Some(attachments) = &self.attachments {
            create_message = create_message.attachments(attachments);
        }
        if let Some(components) = &self.components {
            create_message = create_message.components(components);
        }
        if let Some(embeds) = &self.embeds {
            create_message = create_message.embeds(embeds);
        }
        if let Some(flags) = &self.flags {
            create_message = create_message.flags(*flags);
        }
        create_message
    }

    fn add_to_create_followup<'a>(
        &'a self,
        mut create_followup: CreateFollowup<'a>,
    ) -> CreateFollowup<'a> {
        if let Some(content) = &self.content {
            create_followup = create_followup.content(content);
        }
        if let Some(allowed_mentions) = &self.allowed_mentions {
            create_followup = create_followup.allowed_mentions(Some(allowed_mentions));
        }
        if let Some(attachments) = &self.attachments {
            create_followup = create_followup.attachments(attachments);
        }
        if let Some(components) = &self.components {
            create_followup = create_followup.components(components);
        }
        if let Some(embeds) = &self.embeds {
            create_followup = create_followup.embeds(embeds);
        }
        if let Some(flags) = &self.flags {
            create_followup = create_followup.flags(*flags);
        }
        create_followup
    }

    fn add_to_update_response<'a>(
        &'a self,
        mut update_response: UpdateResponse<'a>,
    ) -> UpdateResponse<'a> {
        if let Some(content) = &self.content {
            update_response = update_response.content(Some(content));
        }
        if let Some(allowed_mentions) = &self.allowed_mentions {
            update_response = update_response.allowed_mentions(Some(allowed_mentions));
        }
        if let Some(attachments) = &self.attachments {
            update_response = update_response.attachments(attachments);
        }
        if let Some(components) = &self.components {
            update_response = update_response.components(Some(components));
        }
        if let Some(embeds) = &self.embeds {
            update_response = update_response.embeds(Some(embeds));
        }
        update_response
    }
}

impl From<CommandResponse> for InteractionResponseData {
    fn from(response: CommandResponse) -> Self {
        let mut builder = InteractionResponseDataBuilder::new();

        if let Some(content) = response.content {
            builder = builder.content(content);
        }
        if let Some(allowed_mentions) = response.allowed_mentions {
            builder = builder.allowed_mentions(allowed_mentions);
        }
        if let Some(attachments) = response.attachments {
            builder = builder.attachments(attachments);
        }
        if let Some(components) = response.components {
            builder = builder.components(components);
        }
        if let Some(embeds) = response.embeds {
            builder = builder.embeds(embeds);
        }
        if let Some(flags) = response.flags {
            builder = builder.flags(flags);
        }

        builder.build()
    }
}

macro_rules! error_embed_helper {
    ($title:expr, $desc:expr) => {{
        twilight_model::channel::message::embed::Embed {
            author: None,
            color: Some(0xFF0000),
            description: $desc,
            fields: Vec::new(),
            footer: None,
            image: None,
            kind: "rich".to_owned(),
            provider: None,
            thumbnail: None,
            timestamp: None,
            title: $title,
            url: None,
            video: None,
        }
    }};
}

macro_rules! error_embed {
    (, $desc:expr) => {
        error_embed_helper!(None, Some($desc))
    };
    ($title:expr,) => {
        error_embed_helper!(Some($title), None)
    };
    ($title:expr, $desc:expr) => {
        error_embed_helper!(Some($title), Some($desc))
    };
}

macro_rules! embed_response {
    ($($embed:expr),+) => {
        CommandResponse {
            embeds: Some(vec![$($embed),+]),
            ..CommandResponse::default()
        }
    };
}

impl From<CommandError> for CommandResponse {
    fn from(error: CommandError) -> Self {
        match error {
            CommandError::NoMsgPermission => Self::default(),
            CommandError::MissingRequirement(req) => match req {
                MissingRequirement::BotOwnerOnly => embed_response!(
                    error_embed![,"This command can only be used by the owner of the bot.".to_owned()]
                ),
                MissingRequirement::General(msg) => embed_response!(error_embed![,msg]),
            },
            CommandError::InvalidArgs(msg) => {
                embed_response!(error_embed!["Invalid Arguments Provided".to_owned(), msg])
            }
            CommandError::InvalidArgsWithUsage(msg, usage) => embed_response!(error_embed![
                "Invalid Arguments Provided".to_owned(),
                format!("{}\n\n**Usage**:\n```\n{}\n```", msg, usage)
            ]),
            CommandError::Cooldown(duration) => {
                let secs = duration.as_secs() + 1;
                embed_response!(
                    error_embed![,format!("Please wait {} second{} before using this command again.", secs, if secs == 1 { "" } else { "s" })]
                )
            }
            CommandError::Generic => {
                embed_response!(error_embed![,"Sorry, there was an unexpected error.".to_owned()])
            }
            CommandError::Specific(msg) => embed_response!(error_embed![, msg]),
        }
    }
}

#[derive(Debug)]
struct CommandResponseBuilder(CommandResponse);

impl CommandResponseBuilder {
    fn new() -> Self {
        Self(CommandResponse::default())
    }

    #[allow(dead_code)]
    fn content(mut self, content: impl Into<String>) -> Self {
        self.0.content = Some(content.into());
        self
    }

    fn embeds(mut self, embeds: Vec<Embed>) -> Self {
        self.0.embeds = Some(embeds);
        self
    }

    // TODO: Use into iter instead, which allows us to use slices but will be more complicated in
    // terms of types
    fn components(mut self, components: Vec<Component>) -> Self {
        self.0.components = Some(components);
        self
    }

    fn flags(mut self, flags: MessageFlags) -> Self {
        self.0.flags = Some(flags);
        self
    }

    fn embed(mut self, embed: Embed) -> Self {
        if let Some(embeds) = self.0.embeds.as_mut() {
            embeds.push(embed);
        } else {
            self.0.embeds = Some(vec![embed]);
        }
        self
    }

    // Add embed with title and description
    fn etd(self, title: impl Into<String>, description: impl Into<String>) -> Self {
        self.embed(make_embed(title, description))
    }

    // Add embed with description
    fn ed(self, description: impl Into<String>) -> Self {
        self.embed(make_titleless_embed(description))
    }

    // Add embed with title
    fn et(self, title: impl Into<String>) -> Self {
        self.embed(make_descless_embed(title))
    }

    fn split_across_embeds<T>(
        self,
        mut first: Embed,
        template: Embed,
        separator: &str,
        description: impl AsRef<[T]>,
    ) -> Result<(Self, usize), SplitAcrossEmbedsError>
    where
        T: AsRef<str>,
    {
        let description = description.as_ref();
        first.description = Some(String::new());
        let mut total = count_embed_chars_no_desc(&first);
        let mut embeds = vec![first];
        let mut last = embeds.last_mut().unwrap();
        let mut add_separator = false;

        let count_temp = count_embed_chars_no_desc(&template);
        let mut index = 0;
        let mut incomplete = false;

        for part in description {
            index += 1;
            let part = part.as_ref();
            if part.len() > 4096 {
                return Err(SplitAcrossEmbedsError::Unsplittable);
            }
            if last.description.as_ref().unwrap().len()
                + (if add_separator { separator.len() } else { 0 })
                + part.len()
                > 4096
            {
                let mut template = template.clone();
                template.description = Some(String::new());
                embeds.push(template);
                last = embeds.last_mut().unwrap();
                add_separator = false;
                total += count_temp;
            }
            let mut_ref = last.description.as_mut().unwrap();
            if add_separator {
                total += separator.len();
            }
            total += part.len();
            if total > 6000 {
                if last.description.as_ref().unwrap().is_empty() {
                    embeds.pop();
                }
                incomplete = true;
                break;
            }
            if add_separator {
                mut_ref.push_str(separator);
            }
            mut_ref.push_str(part);
            add_separator = true;
        }

        // Note how it's impossible to reach 10 embeds before reaching 6000 char limit, so we don't
        // need to check

        Ok((self.embeds(embeds), if incomplete { index } else { 0 }))
    }

    #[must_use = "This method is intended to be used, otherwise the builder is pointless"]
    fn build(self) -> CommandResponse {
        self.0
    }
}

impl From<CommandResponseBuilder> for CommandResponse {
    fn from(builder: CommandResponseBuilder) -> Self {
        builder.build()
    }
}

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
enum SplitAcrossEmbedsError {
    #[error("The inputs provided cannot be split.")]
    Unsplittable,
}

pub fn make_embed(title: impl Into<String>, description: impl Into<String>) -> Embed {
    Embed {
        author: None,
        color: Some(0x22CBE6),
        description: Some(description.into()),
        fields: Vec::new(),
        footer: None,
        image: None,
        kind: "rich".to_owned(),
        provider: None,
        thumbnail: None,
        timestamp: None,
        title: Some(title.into()),
        url: None,
        video: None,
    }
}

pub fn make_titleless_embed(description: impl Into<String>) -> Embed {
    Embed {
        author: None,
        color: Some(0x22CBE6),
        description: Some(description.into()),
        fields: Vec::new(),
        footer: None,
        image: None,
        kind: "rich".to_owned(),
        provider: None,
        thumbnail: None,
        timestamp: None,
        title: None,
        url: None,
        video: None,
    }
}

pub fn make_descless_embed(title: impl Into<String>) -> Embed {
    Embed {
        author: None,
        color: Some(0x22CBE6),
        description: None,
        fields: Vec::new(),
        footer: None,
        image: None,
        kind: "rich".to_owned(),
        provider: None,
        thumbnail: None,
        timestamp: None,
        title: Some(title.into()),
        url: None,
        video: None,
    }
}

pub fn make_empty_embed() -> Embed {
    Embed {
        author: None,
        color: Some(0x22CBE6),
        description: None,
        fields: Vec::new(),
        footer: None,
        image: None,
        kind: "rich".to_owned(),
        provider: None,
        thumbnail: None,
        timestamp: None,
        title: None,
        url: None,
        video: None,
    }
}

pub fn count_embed_chars_no_desc(embed: &Embed) -> usize {
    let mut count = 0;
    if let Some(x) = &embed.title {
        count += x.len();
    }

    if let Some(x) = &embed.author {
        count += x.name.len();
    }

    if let Some(x) = &embed.footer {
        count += x.text.len();
    }

    for field in &embed.fields {
        count += field.name.len() + field.value.len();
    }

    count
}

/// Handle a legacy command (text with prefix command). This implies checking that the user has the
/// necessary permissions, executing the command and sending any error messages.
pub async fn handle_legacy_command(event: Box<MessageCreate>, data: Arc<ShardData>) {
    let prefix = match event.guild_id {
        Some(id) => get_guild_prefix(&data.pool, id.get() as i64)
            .await
            .unwrap_or(DEFAULT_PREFIX.to_owned()),
        None => DEFAULT_PREFIX.to_owned(),
    };

    let command = match event.content.strip_prefix(&prefix) {
        Some(command) => command,
        None => return,
    };

    let mut cmd_iter = command.split_whitespace();

    let keyword = match cmd_iter.next() {
        Some(name) => name,
        None => return,
    };

    let args = cmd_iter.collect::<Vec<&str>>();

    unsafe { exec_legacy_command(&event, &data, keyword, args) }.await
}

pub async fn handle_interaction(event: Box<InteractionCreate>, data: Arc<ShardData>) {
    match event.data {
        Some(InteractionData::ApplicationCommand(ref cmddata))
            if cmddata.kind == CommandType::ChatInput =>
        {
            exec_chatinput_command(&event, &data, &cmddata.name, &cmddata.options).await
        }
        Some(InteractionData::MessageComponent(ref cmpdata))
            if cmpdata.component_type == ComponentType::Button =>
        {
            exec_button(&event, &data, &cmpdata.custom_id).await
        }
        Some(InteractionData::ModalSubmit(ref modaldata)) => {
            exec_modal(&event, &data, &modaldata.custom_id, &modaldata.components).await
        }
        _ => (),
    }
}

async fn exec_button(event: &InteractionCreate, data: &ShardData, custom_id: &str) {
    if custom_id != "verify" || !event.is_guild() || event.member.is_none() {
        let res = respond_ephemeral(event, data, CommandError::Generic.into()).await;
        if let Err(error) = res {
            tracing::error!("1: Error while replying to button: {}", error);
        }
        return;
    }

    let member = event.member.as_ref().unwrap();

    if member.roles.contains(&data.config.verified_role_id) {
        let res = respond_ephemeral(event, data, CommandError::Specific("You are already verified! Run `unlink` to unlink yourself and then `link` to link to a new user.".into()).into()).await;
        if let Err(error) = res {
            tracing::error!("2: Error while replying to button: {}", error);
        }
        return;
    }

    let res = data
        .client
        .interaction(event.application_id)
        .create_response(
            event.id,
            &event.token,
            &InteractionResponse {
                kind: InteractionResponseType::Modal,
                data: Some(
                    InteractionResponseDataBuilder::new()
                        .custom_id("verify")
                        .title("Hypixel Verification")
                        .components([Component::ActionRow(ActionRow {
                            components: vec![Component::TextInput(TextInput {
                                custom_id: "ign".into(),
                                label: "Hypixel IGN (In-Game-Name)".into(),
                                max_length: Some(16),
                                min_length: Some(1),
                                placeholder: Some("ThePlayer789".into()),
                                required: Some(true),
                                style: TextInputStyle::Short,
                                value: None,
                            })],
                        })])
                        .build(),
                ),
            },
        )
        .await;

    if let Err(error) = res {
        tracing::error!(
            "3: Error while replying to button and sending modal: {}",
            error
        );
    }
}

async fn respond_ephemeral(
    event: &InteractionCreate,
    data: &ShardData,
    response: CommandResponse,
) -> Result<(), twilight_http::Error> {
    let mut to_reply: InteractionResponseData = response.into();
    to_reply.flags = Some(MessageFlags::EPHEMERAL);

    data.client
        .interaction(event.application_id)
        .create_response(
            event.id,
            &event.token,
            &InteractionResponse {
                kind: InteractionResponseType::ChannelMessageWithSource,
                data: Some(to_reply),
            },
        )
        .await?;
    Ok(())
}

async fn update_response(
    event: &InteractionCreate,
    data: &ShardData,
    response: CommandResponse,
) -> Result<(), twilight_http::Error> {
    response
        .add_to_update_response(
            data.client
                .interaction(event.application_id)
                .update_response(&event.token),
        )
        .await?;
    Ok(())
}

async fn exec_modal(
    event: &InteractionCreate,
    data: &ShardData,
    custom_id: &str,
    components: &[ModalInteractionDataActionRow],
) {
    let res = data
        .client
        .interaction(event.application_id)
        .create_response(
            event.id,
            &event.token,
            &InteractionResponse {
                kind: InteractionResponseType::DeferredChannelMessageWithSource,
                data: Some(
                    CommandResponseBuilder::new()
                        .flags(MessageFlags::EPHEMERAL)
                        .build()
                        .into(),
                ),
            },
        )
        .await;
    if let Err(error) = res {
        tracing::error!("0: Error while replying to modal: {}", error);
    }

    if custom_id != "verify" || !event.is_guild() || event.member.is_none() || components.is_empty()
    {
        tracing::warn!("Unexpected modal submitted! {}", custom_id);
        let res = update_response(event, data, CommandError::Generic.into()).await;
        if let Err(error) = res {
            tracing::error!("1: Error while replying to modal: {}", error);
        }
        return;
    }

    let member = event.member.as_ref().unwrap();

    if member.user.is_none() {
        let res = update_response(event, data, CommandError::Generic.into()).await;
        if let Err(error) = res {
            tracing::error!("2: Error while replying to modal: {}", error);
        }
        return;
    }

    if member.roles.contains(&data.config.verified_role_id) {
        let res = update_response(event, data, CommandError::Specific("You are already verified! Run `unlink` to unlink yourself and then `link` to link to a new user.".into()).into()).await;
        if let Err(error) = res {
            tracing::error!("3: Error while replying to modal: {}", error);
        }
        return;
    }

    let ign = match components[0].components.first() {
        Some(ModalInteractionDataComponent {
            value: Some(ign), ..
        }) => ign,
        _ => {
            let res = update_response(event, data, CommandError::Generic.into()).await;
            if let Err(error) = res {
                tracing::error!("4: Error while replying to modal: {}", error);
            }
            return;
        }
    };

    let user = member.user.as_ref().unwrap();
    let user_id = user.id;
    let username = &user.name;

    let guild_id = event.guild_id.unwrap();

    let builder = match link_user(user_id, guild_id, ign, username, data).await {
        Ok(builder) => builder,
        Err(e) => {
            let res = update_response(event, data, e.into()).await;
            if let Err(error) = res {
                tracing::error!("5: Error while replying to modal: {}", error);
            }
            return;
        }
    };

    let res = builder
        .flags(MessageFlags::EPHEMERAL)
        .build()
        .add_to_update_response(
            data.client
                .interaction(event.application_id)
                .update_response(&event.token),
        )
        .await;

    if let Err(error) = res {
        tracing::error!(
            "6: Error while replying to modal with a success message: {}",
            error
        );
    }
}

pub async fn register_commands(
    client: InteractionClient<'_>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let commands = get_commands()?;

    client.set_global_commands(&commands).await?;

    tracing::info!("Successfully registered {} commands", commands.len());

    Ok(())
}

#[derive(Debug)]
pub struct CommandCooldown {
    data: HashMap<Id<UserMarker>, Instant>,
    duration: Duration,
}

impl CommandCooldown {
    pub fn new(duration: Duration) -> Self {
        Self {
            data: HashMap::new(),
            duration,
        }
    }

    #[inline]
    fn add(&mut self, user: Id<UserMarker>) {
        self.clean();
        self.data.insert(user, Instant::now() + self.duration);
    }

    #[inline]
    fn clean(&mut self) {
        self.data.retain(|_, time| Instant::now() < *time);
    }

    fn check(&mut self, user: Id<UserMarker>) -> Option<Duration> {
        if let Some(time) = self.data.get(&user) {
            let instant = Instant::now();
            if instant < *time {
                return Some(time.duration_since(instant));
            }
        }
        self.add(user);

        None
    }
}

fn check_owner(cmd: CommandData<'_>) -> Result<(), MissingRequirement> {
    if !OWNERS.contains(&cmd.author_id().unwrap().get()) {
        Err(MissingRequirement::BotOwnerOnly)
    } else {
        Ok(())
    }
}

macro_rules! no_gen_req {
    () => {
        #[inline(always)]
        pub(super) async fn check_gen_requirements(
            _cmd: CommandData<'_>,
            _data: &ShardData,
        ) -> Result<(), MissingRequirement> {
            Ok(())
        }
    };
}

macro_rules! no_req {
    () => {
        #[inline(always)]
        pub(super) async fn check_requirements(
            _cmd: CommandData<'_>,
            _data: &ShardData,
            _args: &Command,
        ) -> Result<(), MissingRequirement> {
            Ok(())
        }
    };
}

include_commands!(updateloop forcelink link linked unlink masslink dryupdate sendverification);
