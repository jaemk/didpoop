use crate::loaders::{CreaturesForUserId, PgLoader};
use async_graphql::{Context, Object, ID};
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

#[Object]
impl CreatureRelation {
    async fn id(&self) -> String {
        self.id.to_string()
    }
    async fn rel_user_id(&self) -> String {
        self.user_id.to_string()
    }
    async fn rel_kind(&self) -> &str {
        &self.kind
    }
    async fn creator_id(&self) -> String {
        self.creator_id.to_string()
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
