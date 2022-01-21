use crate::models::{CreatureRelation, Poop, User};
use crate::{AppError, Result, CONFIG};
use async_graphql::{
    Context, EmptySubscription, ErrorExtensions, FieldResult, Guard, Object, ResultExt,
};
use chrono::Utc;
use sqlx::PgPool;

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
    let token = hex::encode(crate::crypto::rand_bytes(32)?);
    let token_hash = crate::crypto::hmac_sign(&token);
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
        let salt = crate::crypto::new_pw_salt().expect("error generating salt");
        let hash = crate::crypto::derive_password_hash(pw.as_bytes(), salt.as_ref());
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
        let this_hash = crate::crypto::derive_password_hash(
            pw.as_bytes(),
            hex::decode(&user.pw_salt)?.as_ref(),
        );
        if ring::constant_time::verify_slices_are_equal(&user_hash, &this_hash).is_err() {
            return Err(AppError::BadRequest("bad request".into()).extend());
        }
        login_ctx(ctx, &user).await?;
        Ok(user)
    }

    async fn logout(&self, ctx: &Context<'_>) -> bool {
        let token = hex::encode(crate::crypto::rand_bytes(31).unwrap_or_else(|_| vec![0; 31]));
        let token = format!("xx{token}");
        let cookie_str = format_set_cookie(&token);
        ctx.insert_http_header("set-cookie", cookie_str);
        true
    }

    #[graphql(guard = "LoginGuard::new()")]
    async fn create_creature(
        &self,
        ctx: &Context<'_>,
        name: String,
    ) -> FieldResult<CreatureRelation> {
        let user = ctx.data_unchecked::<User>();
        let pool = ctx.data_unchecked::<PgPool>();
        #[derive(sqlx::FromRow)]
        struct CId {
            id: i64,
        }

        let mut tr = pool.begin().await?;
        let c_id: CId = sqlx::query_as(
            "insert into poop.creatures (creator_id, name) values ($1, $2) returning id",
        )
        .bind(&user.id)
        .bind(&name)
        .fetch_one(&mut tr)
        .await?;

        sqlx::query(
            r##"
            insert into poop.creature_access
                (creature_id, user_id, creator_id, kind) values
                ($1, $2, $3, $4)
            "##,
        )
        .bind(&c_id.id)
        .bind(&user.id)
        .bind(&user.id)
        .bind("creator")
        .execute(&mut tr)
        .await?;

        let c: CreatureRelation = sqlx::query_as(
            r##"
            select c.*, ca.user_id, ca.kind from poop.creatures c
                inner join poop.creature_access ca on ca.creature_id = c.id
            where c.id = $1
                and c.deleted is false
                and ca.deleted is false
            "##,
        )
        .bind(&c_id.id)
        .fetch_one(&mut tr)
        .await?;
        tr.commit().await?;
        Ok(c)
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

pub type Schema = async_graphql::Schema<QueryRoot, MutationRoot, EmptySubscription>;
