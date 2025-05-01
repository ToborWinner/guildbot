use crate::models::*;
use deadpool::managed::BuildError;
use diesel::{prelude::*, result::Error::NotFound};
use diesel_async::{
    pooled_connection::AsyncDieselConnectionManager, AsyncPgConnection, RunQueryDsl,
};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

pub type Pool = deadpool::managed::Pool<AsyncDieselConnectionManager<AsyncPgConnection>>;
pub type PoolCreationError = BuildError;

pub async fn create_connection_pool(
    database_url: impl Into<String>,
) -> Result<Pool, PoolCreationError> {
    // create a new connection pool with the default config
    let config = AsyncDieselConnectionManager::<diesel_async::AsyncPgConnection>::new(database_url);
    Pool::builder(config).build()
}

#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("Failed to get connection from connection pool: {0}")]
    PoolError(#[from] deadpool::managed::PoolError<diesel_async::pooled_connection::PoolError>),
    #[error("Failed to execute query: {0}")]
    QueryError(#[from] diesel::result::Error),
}

pub async fn get_guilds(pool: &Pool) -> Result<Vec<Guild>, DatabaseError> {
    use crate::schema::guilds::dsl::*;

    let mut con = pool.get().await?;

    guilds
        .select(Guild::as_select())
        .load(&mut con)
        .await
        .map_err(|e| e.into())
}

pub const DEFAULT_PREFIX: &str = ";";

pub async fn get_guild_prefix(pool: &Pool, guild_id: i64) -> Result<String, DatabaseError> {
    use crate::schema::guilds::dsl::*;

    let mut con = pool.get().await?;

    match guilds
        .select(prefix)
        .filter(id.eq(guild_id))
        .first::<Option<String>>(&mut con)
        .await
    {
        Ok(Some(data)) => Ok(data),
        Ok(None) => Ok(String::from(DEFAULT_PREFIX)),
        Err(NotFound) => Ok(String::from(DEFAULT_PREFIX)),
        Err(e) => Err(e.into()),
    }
}

pub async fn get_linked_user(pool: &Pool, user_id: i64) -> Result<Option<User>, DatabaseError> {
    use crate::schema::users::dsl::*;

    let mut con = pool.get().await?;

    match users
        .select(User::as_select())
        .find(user_id)
        .first(&mut con)
        .await
    {
        Ok(val) => Ok(Some(val)),
        Err(NotFound) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub async fn update_linked_user(
    pool: &Pool,
    user_id: i64,
    username: String,
) -> Result<usize, DatabaseError> {
    use crate::schema::users::dsl::*;

    let mut con = pool.get().await?;

    Ok(diesel::update(users.find(user_id))
        .set(ign.eq(username))
        .execute(&mut con)
        .await?)
}

pub async fn delete_linked_user(pool: &Pool, user_id: i64) -> Result<usize, DatabaseError> {
    use crate::schema::users::dsl::*;

    let mut con = pool.get().await?;

    Ok(diesel::delete(users.find(user_id))
        .execute(&mut con)
        .await?)
}

pub async fn insert_linked_user(
    pool: &Pool,
    user_id: i64,
    username: &str,
    uuid: &str,
) -> Result<usize, DatabaseError> {
    use crate::schema::users;

    let mut con = pool.get().await?;

    let new_user = NewUser {
        id: user_id,
        ign: username,
        uuid,
    };

    Ok(diesel::insert_into(users::table)
        .values(&new_user)
        .execute(&mut con)
        .await?)
}

pub async fn get_linked_users_by_uuids(
    pool: &Pool,
    uuids: Vec<&String>,
) -> Result<Vec<User>, DatabaseError> {
    use crate::schema::users::dsl::*;

    let mut con = pool.get().await?;

    Ok(users.filter(uuid.eq_any(uuids)).load(&mut con).await?)
}

pub async fn get_linked_users_by_ids(
    pool: &Pool,
    ids: Vec<i64>,
) -> Result<Vec<User>, DatabaseError> {
    use crate::schema::users::dsl::*;

    let mut con = pool.get().await?;

    Ok(users.filter(id.eq_any(ids)).load(&mut con).await?)
}

/// # Panics
/// This function panics if establishing a connection fails.
fn establish_connection(database_url: impl AsRef<str>) -> PgConnection {
    PgConnection::establish(database_url.as_ref())
        .unwrap_or_else(|e| panic!("Error connecting to database. Error: {}", e))
}

/// # Panics
/// This function panics if running migrations fails.
fn run_migrations(connection: &mut impl MigrationHarness<diesel::pg::Pg>) {
    pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

    match connection.run_pending_migrations(MIGRATIONS) {
        Ok(migrations) if !migrations.is_empty() => tracing::info!("Migrations: {:#?}", migrations),
        Ok(_) => (),
        Err(e) => panic!("Error running migrations: {}", e),
    }
}

/// # Panics
/// This function panics if setting up the database fails.
pub fn setup_database(database_url: impl AsRef<str>) {
    let mut connection = establish_connection(database_url);
    run_migrations(&mut connection);
}
