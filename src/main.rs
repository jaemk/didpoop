use async_graphql::{
    Context, EmptySubscription, ErrorExtensions, FieldError, FieldResult, Guard, Object, ResultExt,
    ID,
};
use async_graphql_warp::GraphQLResponse;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::collections::HashMap;
use std::convert::Infallible;
use std::io::Read;
use std::net::SocketAddr;
use warp::Filter;

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

    #[error("unauthorized")]
    Unauthorized(String),

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
            AppError::Unauthorized(_) => e.set("code", 401),
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
    // and auth cookie
    pub real_host: Option<String>,
    pub real_domain: Option<String>,
    pub cookie_name: String,
    pub secure_cookie: bool, // only set to false for local dev

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
                s.trim().to_string()
            })
            .unwrap_or_else(|_| "unknown".to_string());
        Self {
            version,
            host: env_or("HOST", "localhost"),
            port: env_or("PORT", "3030").parse().expect("invalid port"),
            real_host: std::env::var("REAL_HOSTNAME").ok(),
            real_domain: std::env::var("REAL_DOMAIN").ok(),
            cookie_name: "poop_auth".to_string(),
            secure_cookie: env_or("SECURE_COOKIE", "true") != "false",
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
    pub fn get_real_domain(&self) -> String {
        self.real_domain
            .clone()
            .unwrap_or_else(|| "localhost".to_string())
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
    pub pw_salt: String,
    pub pw_hash: String,
    pub deleted: bool,
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
    async fn creatures(&self, ctx: &Context<'_>) -> Vec<CreatureRelation> {
        ctx.data_unchecked::<async_graphql::dataloader::DataLoader<PgLoader>>()
            .load_one(CreaturesForUserId(self.id))
            .await
            .unwrap()
            .unwrap_or_else(Vec::new)
    }
    async fn created(&self) -> DateTime<Utc> {
        self.created
    }
    async fn modified(&self) -> DateTime<Utc> {
        self.modified
    }
}

#[derive(Clone, sqlx::FromRow)]
pub struct CreatureRelation {
    pub id: i64,
    pub user_id: i64,
    pub kind: String,
    pub creator_id: i64,
    pub name: String,
    pub deleted: bool,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
}
#[derive(Clone, Hash, PartialEq, Eq)]
pub struct CreaturesForUserId(i64);

#[Object]
impl CreatureRelation {
    async fn id(&self) -> String {
        self.id.to_string()
    }
}

struct PgLoader {
    pool: PgPool,
}
impl PgLoader {
    fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl async_graphql::dataloader::Loader<CreaturesForUserId> for PgLoader {
    type Value = Vec<CreatureRelation>;
    type Error = std::sync::Arc<AppError>;

    async fn load(
        &self,
        keys: &[CreaturesForUserId],
    ) -> std::result::Result<HashMap<CreaturesForUserId, Self::Value>, Self::Error> {
        let query = r##"
            select c.*, ca.user_id, ca.kind from poop.creatures c
                inner join poop.creature_access ca on ca.creature_id = c.id
            where ca.user_id in (select * from unnest($1))
                and ca.deleted is false
                and c.deleted is false
        "##;
        let keys = keys.iter().map(|c| c.0).collect::<Vec<_>>();
        let res: Vec<CreatureRelation> = sqlx::query_as(query)
            .bind(&keys)
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::from)?;
        let res = res.into_iter().fold(HashMap::new(), |mut acc, c| {
            {
                let e = acc
                    .entry(CreaturesForUserId(c.user_id))
                    .or_insert_with(Vec::new);
                e.push(c);
            }
            acc
        });
        Ok(res)
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
            Err(AppError::Unauthorized("Unauthorized".into()).extend())
        } else {
            Ok(())
        }
    }
}

fn format_set_cookie(token: &str) -> String {
    format!(
        "{name}={token}; Domain={domain}; {secure} HttpOnly; Max-Age={max_age}; SameSite=Lax; Path=/",
        name = &CONFIG.cookie_name,
        token = token,
        domain = &CONFIG.get_real_domain(),
        secure = if CONFIG.secure_cookie { "Secure;" } else { "" },
        max_age = &CONFIG.auth_expiration_seconds,
    )
}

async fn login_ctx(ctx: &Context<'_>, user: &User) -> Result<()> {
    let pool = ctx.data_unchecked::<PgPool>();
    let token = hex::encode(crypto::rand_bytes(32)?);
    let token_hash = crypto::hmac_sign(&token);
    let expires = Utc::now()
        .checked_add_signed(chrono::Duration::seconds(
            CONFIG.auth_expiration_seconds as i64,
        ))
        .ok_or_else(|| AppError::from("error calculating auth expiration"))?;
    sqlx::query(
        r##"
        insert into poop.auth_tokens
            (user_id, hash, expires) values ($1, $2, $3)
    "##,
    )
    .bind(&user.id)
    .bind(token_hash)
    .bind(expires)
    .execute(pool)
    .await
    .map_err(AppError::from)?;
    let cookie_str = format_set_cookie(&token);
    ctx.insert_http_header("set-cookie", cookie_str);
    Ok(())
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

        let user = sqlx::query_as(
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
        .extend_err(|_e, ex| ex.set("key", "INVALID_USER_SIGN_UP"))?;

        login_ctx(ctx, &user).await?;
        Ok(user)
    }

    async fn login(&self, ctx: &Context<'_>, email: String, pw: String) -> FieldResult<User> {
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
        if ring::constant_time::verify_slices_are_equal(&user_hash, &this_hash).is_err() {
            return Err(AppError::BadRequest("bad request".into()).extend());
        }
        login_ctx(ctx, &user).await?;
        Ok(user)
    }

    async fn logout(&self, ctx: &Context<'_>) -> bool {
        let token = hex::encode(crypto::rand_bytes(31).unwrap_or_else(|_| vec![0; 31]));
        let token = format!("xx{token}");
        let cookie_str = format_set_cookie(&token);
        ctx.insert_http_header("set-cookie", cookie_str);
        true
    }

    #[graphql(guard = "LoginGuard::new()")]
    async fn do_thing(&self, _ctx: &Context<'_>) -> FieldResult<bool> {
        Ok(true)
    }
}

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    #[graphql(guard = "LoginGuard::new()")]
    async fn poops(&self, _ctx: &Context<'_>) -> Vec<Poop> {
        vec![Poop {
            id: String::from("1").into(),
            maker: String::from("James"),
        }]
    }

    #[graphql(guard = "LoginGuard::new()")]
    async fn user(&self, ctx: &Context<'_>) -> Option<User> {
        let u = ctx.data_opt::<User>();
        u.cloned()
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

    let status = warp::path("status").and(warp::get()).map(move || {
        #[derive(serde::Serialize)]
        struct Status<'a> {
            version: &'a str,
            ok: &'a str,
        }
        serde_json::to_string(&Status {
            version: &CONFIG.version,
            ok: "ok",
        })
        .expect("error serializing status")
    });

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
        .and(warp::filters::cookie::optional(&CONFIG.cookie_name))
        .and(async_graphql_warp::graphql(schema.clone()))
        .and_then(
            |pool: PgPool,
             cookie: Option<String>,
             (schema, mut request): (Schema, async_graphql::Request)| async move {
                if let Some(cookie) = cookie {
                    let hash = crypto::hmac_sign(&cookie);
                    let u: Result<User> = sqlx::query_as(
                        r##"
                        select u.* from poop.users u
                            inner join poop.auth_tokens at on u.id = at.user_id
                        where at.hash = $1
                            and at.deleted is false
                            and at.expires > now()
                            and u.deleted is false"##,
                    )
                    .bind(hash)
                    .fetch_one(&pool)
                    .await
                    .map_err(AppError::from);
                    if let Ok(u) = u {
                        tracing::info!(user = %u.email, user_id = %u.id, "found user for request");
                        request.data.insert(u);
                    }
                }
                request
                    .data
                    .insert(async_graphql::dataloader::DataLoader::new(
                        PgLoader::new(pool),
                        tokio::spawn,
                    ));

                let resp = schema.execute(request).await;
                Ok::<_, Infallible>(GraphQLResponse::from(resp))
            },
        );

    let routes = index
        .or(graphql_post)
        .or(favicon)
        .or(status)
        .with(warp::trace::request());

    tracing::info!(
        version = %CONFIG.version,
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
