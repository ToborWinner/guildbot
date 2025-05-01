use guildbot::{
    commands::register_commands, config::BotConfig, database::setup_database, presence,
    shard_runner, SHUTDOWN,
};
use std::{env, error::Error, path::PathBuf, str::FromStr, sync::atomic::Ordering};
use tokio::signal;
use twilight_gateway::{CloseFrame, Config, Intents};
use twilight_http::Client as HttpClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    // Initialize the tracing subscriber.
    tracing_subscriber::fmt::init();

    // Get the config from systemd LoadCredential/LoadCredentialEncrypted or the config.json in the
    // current working directory.
    let credentials = env::var("CREDENTIALS_DIRECTORY");
    let botconfig: BotConfig = BotConfig::from_json(match credentials {
        Ok(creddir) => {
            let mut path = PathBuf::from_str(&creddir)?;
            path.push("config.json");
            path
        }
        Err(env::VarError::NotPresent) => "config.json".into(),
        Err(x) => return Err(x.into()),
    })?;

    // Setup database if it's not already in the correct state
    setup_database(&botconfig.database_url);

    // HTTP is separate from the gateway, so create a new client.
    let client = HttpClient::new(botconfig.token.clone());

    // Cache the application ID for repeated use later in the process.
    let application_id = {
        let response = client.current_user_application().await?;

        response.model().await?.id
    };

    // Register commands:
    register_commands(client.interaction(application_id)).await?;

    let config = Config::new(
        botconfig.token.clone(),
        Intents::GUILD_MESSAGES
            | Intents::MESSAGE_CONTENT
            | Intents::GUILD_MEMBERS
            | Intents::GUILDS,
    );

    let shards = twilight_gateway::create_recommended(&client, config, |id, builder| {
        builder.presence(presence(id)).build()
    })
    .await?;
    let mut senders = Vec::with_capacity(shards.len());
    let mut tasks = Vec::with_capacity(shards.len());

    tracing::info!(
        "Discord recommended creating {} shard{}",
        shards.len(),
        if shards.len() > 1 { "s" } else { "" }
    );

    for shard in shards {
        senders.push(shard.sender());
        tasks.push(tokio::spawn(shard_runner(shard, botconfig.clone())));
    }

    signal::ctrl_c().await?;
    SHUTDOWN.store(true, Ordering::Relaxed);
    for sender in senders {
        // Ignore error if shard's already shutdown.
        _ = sender.close(CloseFrame::NORMAL);
    }

    for jh in tasks {
        _ = jh.await;
    }

    Ok(())
}
