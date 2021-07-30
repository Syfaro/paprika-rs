use std::{
    collections::{HashMap, HashSet},
    convert::TryInto,
};

use paprika_client::{PaprikaClient, PaprikaCompare, PaprikaId, PaprikaMeal, PaprikaRecipeHash};

#[derive(Debug)]
enum State {
    Added,
    Deleted,
    Changed,
    Equal,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    tracing::info!("starting update");

    let paprika = PaprikaClient::token(std::env::var("PAPRIKA_TOKEN").unwrap())
        .await
        .expect("paprika token must be valid");

    let pool = sqlx::postgres::PgPoolOptions::default()
        .connect(&std::env::var("DATABASE_URL").unwrap())
        .await
        .unwrap();

    tracing::info!("ensuring database is up to date");

    sqlx::migrate!("../migrations")
        .run(&pool)
        .await
        .expect("could not run database migrations");

    check_for_updates(&paprika, &pool).await.unwrap();

    tracing::info!("completed database update");
}

async fn check_for_updates(
    paprika: &PaprikaClient,
    pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<()> {
    let status: std::collections::HashMap<String, i32> = paprika.status().await?.try_into()?;

    for (status, position) in status {
        let database_position =
            sqlx::query_scalar!("SELECT position FROM status WHERE name = $1", status)
                .fetch_optional(pool)
                .await?;

        let needs_update = match database_position {
            Some(database_position) => {
                if position != database_position {
                    tracing::info!(
                        "database out of date, cloud position is {} but database has {}",
                        position,
                        database_position
                    );

                    sqlx::query!(
                        "UPDATE status SET position = $2 WHERE name = $1",
                        status,
                        position
                    )
                    .execute(pool)
                    .await?;

                    true
                } else {
                    tracing::info!("section {} was already up to date", status);

                    false
                }
            }
            None => {
                sqlx::query!(
                    "INSERT INTO status (name, position) VALUES ($1, $2)",
                    status,
                    position
                )
                .execute(pool)
                .await?;

                true
            }
        };

        if needs_update {
            match status.as_str() {
                "recipes" => update_collection::<PaprikaRecipeHash>(&paprika, &pool).await?,
                "meals" => update_collection::<PaprikaMeal>(&paprika, &pool).await?,
                _ => tracing::warn!("other section {} needs update", status),
            }
        }
    }

    Ok(())
}

#[async_trait::async_trait]
trait UpdateItem: Sized {
    async fn existing_items(pool: &sqlx::Pool<sqlx::Postgres>) -> anyhow::Result<Vec<Self>>;
    async fn current_items(paprika: &PaprikaClient) -> anyhow::Result<Vec<Self>>;

    async fn on_add(
        paprika: &PaprikaClient,
        pool: &sqlx::Pool<sqlx::Postgres>,
        new_item: &Self,
    ) -> anyhow::Result<()>;

    async fn on_change(
        paprika: &PaprikaClient,
        pool: &sqlx::Pool<sqlx::Postgres>,
        new_item: &Self,
    ) -> anyhow::Result<()>;

    async fn on_delete(
        paprika: &PaprikaClient,
        pool: &sqlx::Pool<sqlx::Postgres>,
        old_item: &Self,
    ) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
impl UpdateItem for PaprikaRecipeHash {
    async fn existing_items(pool: &sqlx::Pool<sqlx::Postgres>) -> anyhow::Result<Vec<Self>> {
        let recipes = sqlx::query!("SELECT uid, hash FROM recipe")
            .map(|row| PaprikaRecipeHash {
                uid: row.uid,
                hash: row.hash,
            })
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();

        Ok(recipes)
    }

    async fn current_items(paprika: &PaprikaClient) -> anyhow::Result<Vec<Self>> {
        let items = paprika.recipe_list().await?;
        Ok(items)
    }

    async fn on_add(
        paprika: &PaprikaClient,
        pool: &sqlx::Pool<sqlx::Postgres>,
        new_item: &Self,
    ) -> anyhow::Result<()> {
        let recipe = paprika.recipe(&new_item.uid).await?;

        sqlx::query!(
            "INSERT INTO recipe (uid, hash, data) VALUES ($1, $2, $3)",
            recipe.uid,
            recipe.hash,
            serde_json::to_value(&recipe).expect("recipe should be serializable")
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    async fn on_change(
        paprika: &PaprikaClient,
        pool: &sqlx::Pool<sqlx::Postgres>,
        new_item: &Self,
    ) -> anyhow::Result<()> {
        let recipe = paprika.recipe(&new_item.uid).await?;

        sqlx::query!(
            "UPDATE recipe SET hash = $2, data = $3 WHERE uid = $1",
            recipe.uid,
            recipe.hash,
            serde_json::to_value(&recipe).expect("recipe should be serializable")
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    async fn on_delete(
        _paprika: &PaprikaClient,
        pool: &sqlx::Pool<sqlx::Postgres>,
        old_item: &Self,
    ) -> anyhow::Result<()> {
        sqlx::query!("DELETE FROM recipe WHERE uid = $1", old_item.uid)
            .execute(pool)
            .await?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl UpdateItem for PaprikaMeal {
    async fn existing_items(pool: &sqlx::Pool<sqlx::Postgres>) -> anyhow::Result<Vec<Self>> {
        let meals = sqlx::query!("SELECT data FROM meal")
            .map(|row| serde_json::from_value(row.data).unwrap())
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();
        Ok(meals)
    }

    async fn current_items(paprika: &PaprikaClient) -> anyhow::Result<Vec<Self>> {
        let meals = paprika.meal_list().await?;
        Ok(meals)
    }

    async fn on_add(
        _paprika: &PaprikaClient,
        pool: &sqlx::Pool<sqlx::Postgres>,
        new_item: &Self,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            "INSERT INTO meal (uid, recipe_uid, date, data) VALUES ($1, $2, $3, $4)",
            new_item.uid,
            new_item.recipe_uid,
            new_item.date,
            serde_json::to_value(&new_item).expect("meal should be serializable")
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn on_change(
        _paprika: &PaprikaClient,
        pool: &sqlx::Pool<sqlx::Postgres>,
        new_item: &Self,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            "UPDATE meal SET recipe_uid = $2, date = $3, data = $4 WHERE uid = $1",
            new_item.uid,
            new_item.recipe_uid,
            new_item.date,
            serde_json::to_value(&new_item).expect("meal should be serializable")
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn on_delete(
        _paprika: &PaprikaClient,
        pool: &sqlx::Pool<sqlx::Postgres>,
        old_item: &Self,
    ) -> anyhow::Result<()> {
        sqlx::query!("DELETE FROM meal WHERE uid = $1", old_item.uid)
            .execute(pool)
            .await?;
        Ok(())
    }
}

async fn update_collection<C>(
    paprika: &PaprikaClient,
    pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<()>
where
    C: PaprikaId + PaprikaCompare + UpdateItem,
{
    tracing::debug!("updating collection");

    let existing_items: HashMap<String, C> = C::existing_items(pool)
        .await?
        .into_iter()
        .map(|item| (item.paprika_id(), item))
        .collect();
    tracing::debug!("found {} existing items", existing_items.len());

    let current_items: HashMap<String, C> = C::current_items(paprika)
        .await?
        .into_iter()
        .map(|item| (item.paprika_id(), item))
        .collect();
    tracing::debug!("found {} current items", current_items.len());

    let known_items: HashSet<&String> = existing_items
        .iter()
        .chain(current_items.iter())
        .map(|item| item.0)
        .collect();
    tracing::debug!("found {} unique items", known_items.len());

    let item_states: Vec<_> = known_items
        .iter()
        .map(|item| {
            let state = match (existing_items.get(*item), current_items.get(*item)) {
                (Some(existing), Some(current)) => {
                    if existing.paprika_compare(current) {
                        State::Equal
                    } else {
                        State::Changed
                    }
                }
                (Some(_existing), None) => State::Deleted,
                (None, Some(_current)) => State::Added,
                _ => unreachable!("item must have appeared in some state"),
            };

            (item, state)
        })
        .collect();

    for (id, state) in item_states {
        match state {
            State::Added => {
                tracing::info!("item {} was added", id);
                let item = current_items.get(*id).unwrap();
                C::on_add(paprika, pool, item).await?;
            }
            State::Changed => {
                tracing::info!("item {} was changed", id);
                let item = current_items.get(*id).unwrap();
                C::on_change(paprika, pool, item).await?;
            }
            State::Deleted => {
                tracing::info!("item {} was deleted", id);
                let item = existing_items.get(*id).unwrap();
                C::on_delete(paprika, pool, item).await?;
            }
            _ => tracing::info!("item {} was unchanged", id),
        }
    }

    Ok(())
}
