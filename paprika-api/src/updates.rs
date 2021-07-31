use std::{
    collections::{HashMap, HashSet},
    convert::TryInto,
};

use paprika_client::{
    PaprikaAisle, PaprikaClient, PaprikaCompare, PaprikaGroceryItem, PaprikaId, PaprikaMeal,
    PaprikaRecipeHash,
};

#[derive(Debug)]
enum State {
    Added,
    Deleted,
    Changed,
    Equal,
}

/// Attempt to sync database with Paprika's current state.
pub async fn check_for_updates(
    paprika: &PaprikaClient,
    pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<()> {
    let status: HashMap<String, i32> = paprika.status().await?.try_into()?;

    for (name, position) in status {
        let database_position =
            sqlx::query_scalar!("SELECT position FROM status WHERE name = $1", name)
                .fetch_optional(pool)
                .await?;

        let matches_latest =
            matches!(database_position, Some(database_position) if position == database_position);

        let was_updated = if !matches_latest {
            tracing::info!("section {} needs update", name);
            match name.as_str() {
                "recipes" => {
                    update_collection::<PaprikaRecipeHash>(&paprika, &pool).await?;
                    true
                }
                "meals" => {
                    update_collection::<PaprikaMeal>(&paprika, &pool).await?;
                    true
                }
                "groceries" => {
                    update_collection::<PaprikaGroceryItem>(&paprika, &pool).await?;
                    true
                }
                "groceryaisles" => {
                    update_collection::<PaprikaAisle>(&paprika, &pool).await?;
                    true
                }
                _ => {
                    tracing::warn!("other section {} needs update", name);
                    false
                }
            }
        } else {
            tracing::info!("section {} is up to date", name);
            false
        };

        if was_updated {
            tracing::info!("updated {}", name);
            sqlx::query!("INSERT INTO status (name, position) VALUES ($1, $2) ON CONFLICT (name) DO UPDATE SET position = EXCLUDED.position", name, position).execute(pool).await?;
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

/// Update a collection to match Paprika's current state.
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
        let items = paprika.recipes().await?;
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
        let meals = paprika.meals().await?;
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

#[async_trait::async_trait]
impl UpdateItem for PaprikaGroceryItem {
    async fn existing_items(pool: &sqlx::Pool<sqlx::Postgres>) -> anyhow::Result<Vec<Self>> {
        let groceries = sqlx::query!("SELECT data FROM grocery_item")
            .map(|row| serde_json::from_value(row.data).unwrap())
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();
        Ok(groceries)
    }

    async fn current_items(paprika: &PaprikaClient) -> anyhow::Result<Vec<Self>> {
        let groceries = paprika.groceries().await?;
        Ok(groceries)
    }

    async fn on_add(
        _paprika: &PaprikaClient,
        pool: &sqlx::Pool<sqlx::Postgres>,
        new_item: &Self,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            "INSERT INTO grocery_item (uid, data) VALUES ($1, $2)",
            new_item.uid,
            serde_json::to_value(&new_item).expect("grocery item should be serializable")
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
            "UPDATE grocery_item SET data = $2 WHERE uid = $1",
            new_item.uid,
            serde_json::to_value(&new_item).expect("grocery item should be serializable")
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
        sqlx::query!("DELETE FROM grocery_item WHERE uid = $1", old_item.uid)
            .execute(pool)
            .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl UpdateItem for PaprikaAisle {
    async fn existing_items(pool: &sqlx::Pool<sqlx::Postgres>) -> anyhow::Result<Vec<Self>> {
        let aisles = sqlx::query!("SELECT data FROM aisle")
            .map(|row| serde_json::from_value(row.data).unwrap())
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();
        Ok(aisles)
    }

    async fn current_items(paprika: &PaprikaClient) -> anyhow::Result<Vec<Self>> {
        let aisles = paprika.aisles().await?;
        Ok(aisles)
    }

    async fn on_add(
        _paprika: &PaprikaClient,
        pool: &sqlx::Pool<sqlx::Postgres>,
        new_item: &Self,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            "INSERT INTO aisle (uid, name, data) VALUES ($1, $2, $3)",
            new_item.uid,
            new_item.name,
            serde_json::to_value(&new_item).expect("aisles should be serializable")
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
            "UPDATE aisle SET name = $2, data = $3 WHERE uid = $1",
            new_item.uid,
            new_item.name,
            serde_json::to_value(&new_item).expect("aisle should be serializable")
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
        sqlx::query!("DELETE FROM aisle WHERE uid = $1", old_item.uid)
            .execute(pool)
            .await?;
        Ok(())
    }
}
