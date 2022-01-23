use crate::loaders::{AppLoader, CreatureUserId, CreaturesForUserId, PoopsForCreatureId, UserId};
use crate::AppError;
use async_graphql::{Context, ErrorExtensions, FieldResult, Object};
use chrono::{DateTime, Utc};

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
    async fn creatures(&self, ctx: &Context<'_>) -> FieldResult<Vec<CreatureRelation>> {
        let r = ctx
            .data_unchecked::<AppLoader>()
            .load_one(CreaturesForUserId(self.id))
            .await?
            .unwrap_or_else(Vec::new);
        Ok(r)
    }
    async fn created(&self) -> DateTime<Utc> {
        self.created
    }
    async fn modified(&self) -> DateTime<Utc> {
        self.modified
    }
}

#[derive(Clone, sqlx::FromRow)]
pub struct SimpleUser {
    pub id: i64,
    pub name: String,
}
impl std::convert::From<User> for SimpleUser {
    fn from(u: User) -> Self {
        Self {
            id: u.id,
            name: u.name,
        }
    }
}
#[Object]
impl SimpleUser {
    async fn id(&self) -> String {
        self.id.to_string()
    }
    async fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
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

#[Object]
impl CreatureRelation {
    async fn id(&self) -> String {
        self.id.to_string()
    }
    async fn relation(&self) -> &str {
        &self.kind
    }
    async fn creator(&self, ctx: &Context<'_>) -> FieldResult<SimpleUser> {
        let r = ctx
            .data_unchecked::<AppLoader>()
            .load_one(UserId(self.creator_id))
            .await?
            .ok_or_else(|| {
                AppError::E(format!(
                    "missing expected creator {} of poop {}",
                    self.creator_id, self.id
                ))
                .extend()
            })?
            .into();
        Ok(r)
    }
    async fn name(&self) -> &str {
        &self.name
    }
    async fn poops(&self, ctx: &Context<'_>) -> FieldResult<Vec<Poop>> {
        let r = ctx
            .data_unchecked::<AppLoader>()
            .load_one(PoopsForCreatureId(self.id))
            .await?
            .unwrap_or_else(Vec::new);
        Ok(r)
    }
    async fn created(&self) -> DateTime<Utc> {
        self.created
    }
    async fn modified(&self) -> DateTime<Utc> {
        self.modified
    }
}

#[derive(Clone, sqlx::FromRow)]
pub struct Poop {
    pub id: i64,
    pub creator_id: i64,
    pub creature_id: i64,
    pub deleted: bool,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
}

#[Object]
impl Poop {
    async fn id(&self) -> String {
        self.id.to_string()
    }
    async fn creator(&self, ctx: &Context<'_>) -> FieldResult<SimpleUser> {
        let r = ctx
            .data_unchecked::<AppLoader>()
            .load_one(UserId(self.creator_id))
            .await?
            .ok_or_else(|| {
                AppError::E(format!(
                    "missing expected creator {} of poop {}",
                    self.creator_id, self.id
                ))
                .extend()
            })?
            .into();
        Ok(r)
    }
    async fn creature(&self, ctx: &Context<'_>) -> FieldResult<CreatureRelation> {
        let r = ctx
            .data_unchecked::<AppLoader>()
            .load_one(CreatureUserId(self.creature_id, self.creator_id))
            .await?
            .ok_or_else(|| {
                AppError::E(format!(
                    "missing expected creature {} -> user {} relation",
                    self.creature_id, self.creator_id
                ))
                .extend()
            })?;
        Ok(r)
    }
    async fn created(&self) -> DateTime<Utc> {
        self.created
    }
    async fn modified(&self) -> DateTime<Utc> {
        self.modified
    }
}
