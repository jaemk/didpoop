use async_graphql::{
    Context, EmptyMutation, EmptySubscription, ErrorExtensions, FieldError, FieldResult, Guard,
    Object, ResultExt, ID,
};
use async_graphql_warp::GraphQLResponse;
use cached::proc_macro::{cached, once};
use chrono::{Date, DateTime, Utc};
use sqlx::PgPool;
use std::convert::Infallible;
use std::io::Read;
use std::net::SocketAddr;
use warp::{http::Response, Filter};

mod crypto;

lazy_static::lazy_static! {
    pub static ref CONFIG: Config = Config::load();
}

use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("error")]
    E(String),

    #[error("db error, not found")]
    DBNotFound(sqlx::Error),

    #[error("db error")]
    DB(sqlx::Error),

    #[error("forbidden")]
    Forbidden(String),

    #[error("bad request")]
    BadRequest(String),

    #[error("hex error")]
    Hex(#[from] hex::FromHexError),
}
impl AppError {
    fn is_db_not_found(&self) -> bool {
        matches!(*self, Self::DBNotFound(_))
    }
}
impl From<&str> for AppError {
    fn from(s: &str) -> AppError {
        AppError::E(s.to_string())
    }
}
impl From<String> for AppError {
    fn from(s: String) -> AppError {
        AppError::E(s)
    }
}
impl From<sqlx::Error> for AppError {
    fn from(s: sqlx::Error) -> AppError {
        match s {
            sqlx::Error::RowNotFound => AppError::DBNotFound(s),
            _ => AppError::DB(s),
        }
    }
}
impl ErrorExtensions for AppError {
    fn extend(&self) -> FieldError {
        self.extend_with(|err, e| match err {
            AppError::E(_) => e.set("code", "500"),
            AppError::DB(_) => e.set("code", 500),
            AppError::DBNotFound(_) => e.set("code", 404),
            AppError::Forbidden(_) => e.set("code", 403),
            AppError::BadRequest(_) => e.set("code", 400),
            AppError::Hex(_) => e.set("code", 500),
        })
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

fn env_or(k: &str, default: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| default.to_string())
}

pub struct Config {
    pub version: String,

    // host to listen on, defaults to localhost
    pub host: String,
    pub port: u16,

    // used for building redirects, https://didpoop.com
    pub real_host: Option<String>,

    pub log_level: String,

    // db config
    pub db_url: String,
    pub db_max_connections: u32,

    // key used for encrypting things
    pub encryption_key: String,

    // key used for signing/hashing things
    pub signing_key: String,

    pub auth_expiration_seconds: u32,
}
impl Config {
    pub fn load() -> Self {
        let version = std::fs::File::open("commit_hash.txt")
            .map(|mut f| {
                let mut s = String::new();
                f.read_to_string(&mut s).expect("Error reading commit_hash");
                s
            })
            .unwrap_or_else(|_| "unknown".to_string());
        Self {
            version,
            host: env_or("HOST", "localhost"),
            port: env_or("PORT", "3030").parse().expect("invalid port"),
            real_host: std::env::var("REAL_HOSTNAME").ok(),
            log_level: env_or("LOG_LEVEL", "info"),
            db_url: env_or("DATABASE_URL", "error"),
            db_max_connections: env_or("DATABASE_MAX_CONNECTIONS", "5")
                .parse()
                .expect("invalid DATABASE_MAX_CONNECTIONS"),
            // 60 * 24 * 30
            auth_expiration_seconds: env_or("AUTH_EXPIRATION_SECONDS", "43200")
                .parse()
                .expect("invalid auth_expiration_seconds"),
            encryption_key: env_or("ENCRYPTION_KEY", "01234567890123456789012345678901"),
            signing_key: env_or("SIGNING_KEY", "01234567890123456789012345678901"),
        }
    }
    pub fn initialize(&self) {
        tracing::info!(
            version = %CONFIG.version,
            host = %CONFIG.host,
            port = %CONFIG.port,
            real_host = ?CONFIG.real_host,
            db_max_connections = %CONFIG.db_max_connections,
            log_level = %CONFIG.log_level,
            auth_expiration_seconds = %CONFIG.auth_expiration_seconds,
            "initialized config",
        );
    }
    pub fn get_host_port(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
    pub fn get_real_host(&self) -> String {
        self.real_host
            .clone()
            .unwrap_or_else(|| format!("http://{}:{}", self.host, self.port))
    }
    pub fn get_login_url(&self) -> String {
        format!("{}/login", self.get_real_host())
    }
}

#[derive(Clone, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub email: String,
    pub name: String,
    pub pw_hash: String,
    pub pw_salt: String,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
}

#[Object]
impl User {
    async fn id(&self) -> String {
        self.id.to_string()
    }
    async fn email(&self) -> &str {
        &self.email
    }
    async fn name(&self) -> &str {
        &self.name
    }
    async fn created(&self) -> DateTime<Utc> {
        self.created
    }
    async fn modified(&self) -> DateTime<Utc> {
        self.modified
    }
}

#[derive(Clone)]
pub struct Poop {
    pub id: ID,
    pub maker: String,
}

#[Object]
impl Poop {
    async fn id(&self) -> &str {
        &self.id
    }

    async fn maker(&self) -> &str {
        &self.maker
    }
}

struct LoginGuard;

impl LoginGuard {
    fn new() -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl Guard for LoginGuard {
    async fn check(&self, ctx: &Context<'_>) -> FieldResult<()> {
        if ctx.data_opt::<User>().is_none() {
            Err(AppError::Forbidden("Forbidden".into()).extend())
        } else {
            Ok(())
        }
    }
}

pub struct MutationRoot;

#[Object]
impl MutationRoot {
    async fn sign_up(
        &self,
        ctx: &Context<'_>,
        email: String,
        name: String,
        pw: String,
    ) -> FieldResult<User> {
        let salt = crypto::new_pw_salt().expect("error generating salt");
        let hash = crypto::derive_password_hash(pw.as_bytes(), salt.as_ref());
        let salt = hex::encode(salt);
        let hash = hex::encode(hash);
        let pool = ctx.data_unchecked::<PgPool>();
        sqlx::query_as(
            r##"
            insert into poop.users (name, email, pw_salt, pw_hash)
                values ($1, $2, $3, $4)
                returning *
        "##,
        )
        .bind(name)
        .bind(email)
        .bind(salt)
        .bind(hash)
        .fetch_one(pool)
        .await
        .map_err(AppError::from)
        .extend_err(|_e, ex| ex.set("key", "INVALID_USER_SIGN_UP"))
    }

    async fn login(&self, ctx: &Context<'_>, email: String, pw: String) -> FieldResult<User> {
        // set auth cookie
        let pool = ctx.data_unchecked::<PgPool>();
        let user: User =
            sqlx::query_as("select * from poop.users where email = $1 and deleted is false")
                .bind(email)
                .fetch_one(pool)
                .await
                .map_err(AppError::from)
                .map_err(|e| {
                    if e.is_db_not_found() {
                        AppError::BadRequest("bad request".into())
                    } else {
                        e
                    }
                })?;
        let user_hash = hex::decode(&user.pw_hash)?;
        let this_hash =
            crypto::derive_password_hash(pw.as_bytes(), hex::decode(&user.pw_salt)?.as_ref());
        if !ring::constant_time::verify_slices_are_equal(&user_hash, &this_hash).is_ok() {
            return Err(AppError::BadRequest("bad request".into()).extend());
        }
        ctx.insert_http_header("set-cookie", format!("poop_auth=test"));
        Ok(user)
    }

    #[graphql(guard = "LoginGuard::new()")]
    async fn do_thing(&self, ctx: &Context<'_>) -> FieldResult<bool> {
        Ok(true)
    }
}

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn poops(&self, ctx: &Context<'_>) -> Vec<Poop> {
        vec![Poop {
            id: String::from("1").into(),
            maker: String::from("James"),
        }]
    }
    async fn user(&self, ctx: &Context<'_>) -> FieldResult<User> {
        let pool = ctx.data_unchecked::<PgPool>();
        sqlx::query_as("select * from poop.users limit 1")
            .fetch_one(pool)
            .await
            .map_err(AppError::from)
            .extend_err(|_e, ex| ex.set("context", "no current user"))
    }
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("Error running server: {e}");
        std::process::exit(1);
    }
}

pub type Schema = async_graphql::Schema<QueryRoot, MutationRoot, EmptySubscription>;

async fn run() -> Result<()> {
    dotenv::dotenv().ok();

    let addr = CONFIG.get_host_port();
    let filter = tracing_subscriber::filter::EnvFilter::new(&CONFIG.log_level);
    tracing_subscriber::fmt().with_env_filter(filter).init();
    let pool = sqlx::PgPool::connect(&CONFIG.db_url).await?;

    let favicon = warp::path("favicon.ico")
        .and(warp::get())
        .and(warp::fs::file("static/think.jpg"));

    let index = warp::any().and(warp::path::end()).map(|| "hello");

    let schema = async_graphql::Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(pool.clone())
        .finish();

    let graphql_post = warp::path!("api" / "graphql")
        .and(warp::path::end())
        .map(move || pool.clone())
        .and(warp::filters::cookie::optional("poop_auth"))
        .and(async_graphql_warp::graphql(schema.clone()))
        .and_then(
            |pool: PgPool,
             cookie: Option<String>,
             (schema, mut request): (Schema, async_graphql::Request)| async move {
                if let Some(cookie) = cookie {
                    tracing::info!("found cookie, looking for user");
                    let hash = crypto::hmac_sign(&cookie);
                    let u: Result<User> = sqlx::query_as(
                        r##"
                        select * from poop.auth_tokens
                            where hash = $1
                                and deleted is false
                                and expires > now()"##,
                    )
                    .bind(hash)
                    .fetch_one(&pool)
                    .await
                    .map_err(AppError::from);
                    if let Ok(u) = u {
                        request.data.insert(u);
                    }
                }

                let resp = schema.execute(request).await;
                Ok::<_, Infallible>(GraphQLResponse::from(resp))
            },
        );

    let routes = index
        .or(graphql_post)
        .or(favicon)
        .with(warp::trace::request());

    tracing::info!(
        addr = %addr,
        "starting server",
    );
    warp::serve(routes)
        .run(
            addr.parse::<SocketAddr>()
                .map_err(|e| format!("invalid host/port: {addr}, {e}"))
                .unwrap(),
        )
        .await;
    Ok(())
}
