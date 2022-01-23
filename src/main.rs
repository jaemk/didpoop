use async_graphql::{dataloader::HashMapCache, EmptySubscription};
use async_graphql_warp::GraphQLResponse;
use sqlx::PgPool;
use std::convert::Infallible;
use std::net::SocketAddr;
use warp::{hyper::Method, Filter};

mod config;
mod crypto;
mod error;
mod loaders;
mod models;
mod schema;

use error::{AppError, Result};
use loaders::PgLoader;
use models::User;
use schema::{MutationRoot, QueryRoot, Schema};

lazy_static::lazy_static! {
    pub static ref CONFIG: config::Config = config::Config::load();
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("Error running server: {e}");
        std::process::exit(1);
    }
}

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
        .and(warp::post())
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
                let loader = async_graphql::dataloader::DataLoader::with_cache(
                    PgLoader::new(pool),
                    tokio::spawn,
                    HashMapCache::default(),
                );
                request.data.insert(loader);

                let resp = schema.execute(request).await;
                Ok::<_, Infallible>(GraphQLResponse::from(resp))
            },
        );

    let index_options = warp::path::end().and(warp::options()).map(warp::reply);

    let graphql_options = warp::path!("api" / "graphql")
        .and(warp::path::end())
        .and(warp::options())
        .map(warp::reply);

    let cors = warp::cors()
        .allow_methods(&[Method::GET, Method::POST])
        .allow_headers(["cookie", "content-type"])
        .allow_origins([
            "http://localhost:3000",
            "http://localhost:3003",
            "https://didpoop.com",
        ]);
    let routes = index
        .or(index_options)
        .or(graphql_post)
        .or(graphql_options)
        .or(favicon)
        .or(status)
        .with(cors)
        .with(warp::trace::request());

    if !CONFIG.secure_cookie {
        tracing::warn!("*** SECURE COOKIE IS DISABLED ***");
    }
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
