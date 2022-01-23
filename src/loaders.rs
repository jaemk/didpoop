use crate::models::{CreatureRelation, Poop, User};
use crate::AppError;
use async_graphql::dataloader::{DataLoader, HashMapCache};
use sqlx::PgPool;
use std::collections::HashMap;

pub struct PgLoader {
    pool: PgPool,
}
impl PgLoader {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
pub type AppLoader = DataLoader<PgLoader, HashMapCache>;

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct UserId(pub i64);

#[async_trait::async_trait]
impl async_graphql::dataloader::Loader<UserId> for PgLoader {
    type Value = User;
    type Error = std::sync::Arc<AppError>;

    async fn load(
        &self,
        keys: &[UserId],
    ) -> std::result::Result<HashMap<UserId, Self::Value>, Self::Error> {
        let query = r##"
            select * from poop.users where id in (select * from unnest($1))
        "##;
        let u_ids = keys.iter().map(|c| c.0).collect::<Vec<_>>();
        let res: Vec<User> = sqlx::query_as(query)
            .bind(&u_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::from)?;
        let res = res.into_iter().fold(HashMap::new(), |mut acc, u| {
            acc.insert(UserId(u.id), u);
            acc
        });
        Ok(res)
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct CreatureUserId(pub i64, pub i64);

#[async_trait::async_trait]
impl async_graphql::dataloader::Loader<CreatureUserId> for PgLoader {
    type Value = CreatureRelation;
    type Error = std::sync::Arc<AppError>;

    async fn load(
        &self,
        keys: &[CreatureUserId],
    ) -> std::result::Result<HashMap<CreatureUserId, Self::Value>, Self::Error> {
        let query = r##"
            select c.*, ca.user_id, ca.kind from poop.creatures c
                inner join poop.creature_access ca on ca.creature_id = c.id
            where c.deleted is false
                and ca.deleted is false
                and (
                    ca.user_id in (select * from unnest($1))
                    or ca.creature_id in (select * from unnest($2))
                )
        "##;
        let c_ids = keys.iter().map(|c| c.0).collect::<Vec<_>>();
        let u_ids = keys.iter().map(|c| c.1).collect::<Vec<_>>();
        let res: Vec<CreatureRelation> = sqlx::query_as(query)
            .bind(&u_ids)
            .bind(&c_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::from)?;
        let res = res.into_iter().fold(HashMap::new(), |mut acc, c| {
            acc.insert(CreatureUserId(c.id, c.user_id), c);
            acc
        });
        Ok(res)
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct CreaturesForUserId(pub i64);

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

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct PoopsForCreatureId(pub i64);

#[async_trait::async_trait]
impl async_graphql::dataloader::Loader<PoopsForCreatureId> for PgLoader {
    type Value = Vec<Poop>;
    type Error = std::sync::Arc<AppError>;

    async fn load(
        &self,
        keys: &[PoopsForCreatureId],
    ) -> std::result::Result<HashMap<PoopsForCreatureId, Self::Value>, Self::Error> {
        let query = r##"
            select p.* from poop.poops p
            where p.creature_id in (select * from unnest($1))
                and p.deleted is false
                order by p.created desc
        "##;
        let keys = keys.iter().map(|c| c.0).collect::<Vec<_>>();
        let res: Vec<Poop> = sqlx::query_as(query)
            .bind(&keys)
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::from)?;
        let res = res.into_iter().fold(HashMap::new(), |mut acc, p| {
            {
                let e = acc
                    .entry(PoopsForCreatureId(p.creature_id))
                    .or_insert_with(Vec::new);
                e.push(p);
            }
            acc
        });
        Ok(res)
    }
}
