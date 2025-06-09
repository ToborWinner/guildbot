use commands::{handle_interaction, handle_legacy_command, CommandCooldownList};
use config::BotConfig;
use database::{create_connection_pool, Pool, PoolCreationError};
use minecraft::{handle_member_add, update_loop, HypixelAPI};
use std::{
    error::Error,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use twilight_cache_inmemory::{DefaultInMemoryCache, InMemoryCache, ResourceType};
use twilight_gateway::{
    error::ReceiveMessageErrorType, Event, EventTypeFlags, Shard, ShardId, StreamExt,
};
use twilight_http::Client;
use twilight_model::gateway::{
    payload::outgoing::update_presence::UpdatePresencePayload,
    presence::{ActivityType, MinimalActivity, Status},
};

pub mod commands;
pub mod config;
pub mod database;
mod minecraft;
pub mod models;
pub mod schema;

const OWNERS: [u64; 1] = [887046293669707797];

pub static SHUTDOWN: AtomicBool = AtomicBool::new(false);

pub struct ShardData {
    pub client: Client,
    pub cache: InMemoryCache,
    pub pool: Pool,
    pub command_cooldowns: CommandCooldownList,
    pub reqwest_client: reqwest::Client,
    pub hypixel: HypixelAPI,
    pub paused_loop: AtomicBool,
    pub config: BotConfig,
}

impl ShardData {
    pub fn new(client: Client, cache: InMemoryCache, pool: Pool, config: BotConfig) -> Self {
        let reqwest_client = reqwest::Client::new();
        Self {
            client,
            cache,
            pool,
            command_cooldowns: CommandCooldownList::new(),
            reqwest_client: reqwest_client.clone(),
            hypixel: HypixelAPI::new(config.api_key.clone(), reqwest_client),
            paused_loop: AtomicBool::new(false),
            config,
        }
    }

    pub async fn create(config: BotConfig) -> Result<Self, PoolCreationError> {
        let client = Client::new(config.token.clone());
        let cache = DefaultInMemoryCache::builder()
            .resource_types(ResourceType::MESSAGE)
            .build();
        let pool = create_connection_pool(config.database_url.clone()).await?;
        let reqwest_client = reqwest::Client::new();

        Ok(Self {
            client,
            cache,
            pool,
            command_cooldowns: CommandCooldownList::new(),
            reqwest_client: reqwest_client.clone(),
            hypixel: HypixelAPI::new(config.api_key.clone(), reqwest_client),
            paused_loop: AtomicBool::new(false),
            config,
        })
    }
}

pub async fn shard_runner(mut shard: Shard, config: BotConfig) {
    tracing::info!("({}) Starting shard...", shard.id().number());

    let shard_data = match ShardData::create(config).await {
        Ok(data) => data,
        Err(source) => {
            tracing::error!(?source, "error creating shard data");

            // TODO: Handle runners failing properly.
            panic!("Failed to create shard data: {}", source);
        }
    };

    let data = Arc::new(shard_data);

    tokio::spawn(update_loop(data.clone()));

    while let Some(item) = shard.next_event(EventTypeFlags::all()).await {
        let event = match item {
            Ok(Event::GatewayClose(_)) if SHUTDOWN.load(Ordering::Relaxed) => {
                tracing::info!("Shutdown triggered by gateway close.");
                break;
            }
            Ok(event) => event,
            Err(source)
                if SHUTDOWN.load(Ordering::Relaxed)
                    && matches!(source.kind(), ReceiveMessageErrorType::Reconnect) =>
            {
                tracing::info!("Shutdown triggered by reconnect.");
                break;
            }
            Err(source) => {
                tracing::warn!(?source, "error receiving event");

                continue;
            }
        };

        // Update the cache with the event.
        data.cache.update(&event);

        // Handle the event.
        tokio::spawn(handle_event(event, data.clone()));
    }

    tracing::info!("({}) Shutdown successful", shard.id().number());
}

async fn handle_event(
    event: Event,
    data: Arc<ShardData>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match event {
        Event::Ready(event) => {
            if let Some(shard) = event.shard {
                tracing::info!(
                    "({}) READY! Connected as {}",
                    shard.number(),
                    event.user.name
                );
            } else {
                tracing::info!("READY! Connected as {}", event.user.name);
            }
        }
        Event::MessageCreate(msg) => handle_legacy_command(msg, data).await,
        Event::InteractionCreate(interaction) => handle_interaction(interaction, data).await,
        Event::MemberAdd(member_add) => handle_member_add(member_add, data).await,
        // Other events here...
        _ => {}
    }

    Ok(())
}

pub fn presence(id: ShardId) -> UpdatePresencePayload {
    let activity = MinimalActivity {
        kind: ActivityType::Playing,
        name: format!("with Rust & Twilight ({})", id.number()),
        url: None,
    };

    UpdatePresencePayload {
        activities: vec![activity.into()],
        afk: false,
        since: None,
        status: Status::Online,
    }
}
