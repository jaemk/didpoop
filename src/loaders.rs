use crate::models::CreatureRelation;
use crate::AppError;
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
