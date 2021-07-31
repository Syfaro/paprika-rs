use std::{
    collections::{HashMap, HashSet},
    convert::TryInto,
};

use paprika_client::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum State {
    Added,
    Deleted,
    Changed,
    Equal,
}

/// Attempt to sync database with Paprika's current state.
pub async fn check_for_updates(
    paprika: &PaprikaClient,
    pool: &sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<HashMap<State, usize>> {
    let status: HashMap<String, i32> = paprika.status().await?.try_into()?;

    let mut changes = HashMap::with_capacity(4);

    for (name, position) in status {
        let database_position =
            sqlx::query_scalar!("SELECT position FROM status WHERE name = $1", name)
                .fetch_optional(pool)
                .await?;

        let matches_latest =
            matches!(database_position, Some(database_position) if position == database_position);

        if !matches_latest {
            tracing::info!("section {} needs update", name);
            let item_changes = match name.as_str() {
                "menus" => update_collection::<PaprikaMenu>(&paprika, &pool).await?,
                "photos" => update_collection::<PaprikaPhoto>(&paprika, &pool).await?,
                "mealtypes" => update_collection::<PaprikaMealType>(&paprika, &pool).await?,
                "recipes" => update_collection::<PaprikaRecipeHash>(&paprika, &pool).await?,
                "pantry" => update_collection::<PaprikaPantryItem>(&paprika, &pool).await?,
                "meals" => update_collection::<PaprikaMeal>(&paprika, &pool).await?,
                "groceryingredients" => {
                    update_collection::<PaprikaGroceryIngredient>(&paprika, &pool).await?
                }
                "groceries" => update_collection::<PaprikaGroceryItem>(&paprika, &pool).await?,
                "groceryaisles" => update_collection::<PaprikaAisle>(&paprika, &pool).await?,
                "grocerylists" => update_collection::<PaprikaGroceryList>(&paprika, &pool).await?,
                "bookmarks" => update_collection::<PaprikaBookmark>(&paprika, &pool).await?,
                "menuitems" => update_collection::<PaprikaMenuItem>(&paprika, &pool).await?,
                "categories" => update_collection::<PaprikaCategory>(&paprika, &pool).await?,
                _ => unreachable!("unknown paprika changed item"),
            };

            for (state, count) in item_changes {
                *changes.entry(state).or_default() += count;
            }
        } else {
            tracing::info!("section {} is up to date", name);
        };

        tracing::info!("updated {}", name);
        sqlx::query!("INSERT INTO status (name, position) VALUES ($1, $2) ON CONFLICT (name) DO UPDATE SET position = EXCLUDED.position", name, position).execute(pool).await?;
    }

    tracing::debug!("observed changes: {:?}", changes);

    Ok(changes)
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
) -> anyhow::Result<HashMap<State, usize>>
where
    C: PaprikaId + Eq + UpdateItem,
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
                    if existing.eq(current) {
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

    let mut changes: HashMap<State, usize> = HashMap::with_capacity(4);

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

        *changes.entry(state).or_default() += 1;
    }

    Ok(changes)
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
            serde_json::to_value(&recipe)?
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
            serde_json::to_value(&recipe)?
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
            serde_json::to_value(&new_item)?
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
            serde_json::to_value(&new_item)?
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
            serde_json::to_value(&new_item)?
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
            serde_json::to_value(&new_item)?
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
            serde_json::to_value(&new_item)?
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
            serde_json::to_value(&new_item)?
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

#[async_trait::async_trait]
impl UpdateItem for PaprikaMenu {
    async fn existing_items(pool: &sqlx::Pool<sqlx::Postgres>) -> anyhow::Result<Vec<Self>> {
        let menus = sqlx::query!("SELECT data FROM menu")
            .map(|row| serde_json::from_value(row.data).unwrap())
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();
        Ok(menus)
    }

    async fn current_items(paprika: &PaprikaClient) -> anyhow::Result<Vec<Self>> {
        let menus = paprika.menus().await?;
        Ok(menus)
    }

    async fn on_add(
        _paprika: &PaprikaClient,
        pool: &sqlx::Pool<sqlx::Postgres>,
        new_item: &Self,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            "INSERT INTO menu (uid, data) VALUES ($1, $2)",
            new_item.uid,
            serde_json::to_value(&new_item)?
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
            "UPDATE menu SET data = $2 WHERE uid = $1",
            new_item.uid,
            serde_json::to_value(&new_item)?
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
        sqlx::query!("DELETE FROM menu WHERE uid = $1", old_item.uid)
            .execute(pool)
            .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl UpdateItem for PaprikaPhoto {
    async fn existing_items(pool: &sqlx::Pool<sqlx::Postgres>) -> anyhow::Result<Vec<Self>> {
        let photos = sqlx::query!("SELECT data FROM photo")
            .map(|row| serde_json::from_value(row.data).unwrap())
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();
        Ok(photos)
    }

    async fn current_items(paprika: &PaprikaClient) -> anyhow::Result<Vec<Self>> {
        let photos = paprika.photos().await?;
        Ok(photos)
    }

    async fn on_add(
        _paprika: &PaprikaClient,
        pool: &sqlx::Pool<sqlx::Postgres>,
        new_item: &Self,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            "INSERT INTO photo (uid, data) VALUES ($1, $2)",
            new_item.uid,
            serde_json::to_value(&new_item)?
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
            "UPDATE photo SET data = $2 WHERE uid = $1",
            new_item.uid,
            serde_json::to_value(&new_item)?
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
        sqlx::query!("DELETE FROM photo WHERE uid = $1", old_item.uid)
            .execute(pool)
            .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl UpdateItem for PaprikaMealType {
    async fn existing_items(pool: &sqlx::Pool<sqlx::Postgres>) -> anyhow::Result<Vec<Self>> {
        let meal_types = sqlx::query!("SELECT data FROM meal_type")
            .map(|row| serde_json::from_value(row.data).unwrap())
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();
        Ok(meal_types)
    }

    async fn current_items(paprika: &PaprikaClient) -> anyhow::Result<Vec<Self>> {
        let meal_types = paprika.meal_types().await?;
        Ok(meal_types)
    }

    async fn on_add(
        _paprika: &PaprikaClient,
        pool: &sqlx::Pool<sqlx::Postgres>,
        new_item: &Self,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            "INSERT INTO meal_type (uid, data) VALUES ($1, $2)",
            new_item.uid,
            serde_json::to_value(&new_item)?
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
            "UPDATE meal_type SET data = $2 WHERE uid = $1",
            new_item.uid,
            serde_json::to_value(&new_item)?
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
        sqlx::query!("DELETE FROM meal_type WHERE uid = $1", old_item.uid)
            .execute(pool)
            .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl UpdateItem for PaprikaPantryItem {
    async fn existing_items(pool: &sqlx::Pool<sqlx::Postgres>) -> anyhow::Result<Vec<Self>> {
        let pantry_items = sqlx::query!("SELECT data FROM pantry_item")
            .map(|row| serde_json::from_value(row.data).unwrap())
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();
        Ok(pantry_items)
    }

    async fn current_items(paprika: &PaprikaClient) -> anyhow::Result<Vec<Self>> {
        let pantry_items = paprika.pantry_items().await?;
        Ok(pantry_items)
    }

    async fn on_add(
        _paprika: &PaprikaClient,
        pool: &sqlx::Pool<sqlx::Postgres>,
        new_item: &Self,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            "INSERT INTO pantry_item (uid, data) VALUES ($1, $2)",
            new_item.uid,
            serde_json::to_value(&new_item)?
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
            "UPDATE pantry_item SET data = $2 WHERE uid = $1",
            new_item.uid,
            serde_json::to_value(&new_item)?
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
        sqlx::query!("DELETE FROM pantry_item WHERE uid = $1", old_item.uid)
            .execute(pool)
            .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl UpdateItem for PaprikaGroceryIngredient {
    async fn existing_items(pool: &sqlx::Pool<sqlx::Postgres>) -> anyhow::Result<Vec<Self>> {
        let grocery_ingredients = sqlx::query!("SELECT data FROM grocery_ingredient")
            .map(|row| serde_json::from_value(row.data).unwrap())
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();
        Ok(grocery_ingredients)
    }

    async fn current_items(paprika: &PaprikaClient) -> anyhow::Result<Vec<Self>> {
        let grocery_ingredients = paprika.grocery_ingredients().await?;
        Ok(grocery_ingredients)
    }

    async fn on_add(
        _paprika: &PaprikaClient,
        pool: &sqlx::Pool<sqlx::Postgres>,
        new_item: &Self,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            "INSERT INTO grocery_ingredient (uid, data) VALUES ($1, $2)",
            new_item.uid,
            serde_json::to_value(&new_item)?
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
            "UPDATE grocery_ingredient SET data = $2 WHERE uid = $1",
            new_item.uid,
            serde_json::to_value(&new_item)?
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
        sqlx::query!(
            "DELETE FROM grocery_ingredient WHERE uid = $1",
            old_item.uid
        )
        .execute(pool)
        .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl UpdateItem for PaprikaGroceryList {
    async fn existing_items(pool: &sqlx::Pool<sqlx::Postgres>) -> anyhow::Result<Vec<Self>> {
        let grocery_lists = sqlx::query!("SELECT data FROM grocery_list")
            .map(|row| serde_json::from_value(row.data).unwrap())
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();
        Ok(grocery_lists)
    }

    async fn current_items(paprika: &PaprikaClient) -> anyhow::Result<Vec<Self>> {
        let grocery_lists = paprika.grocery_lists().await?;
        Ok(grocery_lists)
    }

    async fn on_add(
        _paprika: &PaprikaClient,
        pool: &sqlx::Pool<sqlx::Postgres>,
        new_item: &Self,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            "INSERT INTO grocery_list (uid, data) VALUES ($1, $2)",
            new_item.uid,
            serde_json::to_value(&new_item)?
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
            "UPDATE grocery_list SET data = $2 WHERE uid = $1",
            new_item.uid,
            serde_json::to_value(&new_item)?
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
        sqlx::query!("DELETE FROM grocery_list WHERE uid = $1", old_item.uid)
            .execute(pool)
            .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl UpdateItem for PaprikaBookmark {
    async fn existing_items(pool: &sqlx::Pool<sqlx::Postgres>) -> anyhow::Result<Vec<Self>> {
        let bookmarks = sqlx::query!("SELECT data FROM bookmark")
            .map(|row| serde_json::from_value(row.data).unwrap())
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();
        Ok(bookmarks)
    }

    async fn current_items(paprika: &PaprikaClient) -> anyhow::Result<Vec<Self>> {
        let bookmarks = paprika.bookmarks().await?;
        Ok(bookmarks)
    }

    async fn on_add(
        _paprika: &PaprikaClient,
        pool: &sqlx::Pool<sqlx::Postgres>,
        new_item: &Self,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            "INSERT INTO bookmark (uid, data) VALUES ($1, $2)",
            new_item.uid,
            serde_json::to_value(&new_item)?
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
            "UPDATE bookmark SET data = $2 WHERE uid = $1",
            new_item.uid,
            serde_json::to_value(&new_item)?
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
        sqlx::query!("DELETE FROM bookmark WHERE uid = $1", old_item.uid)
            .execute(pool)
            .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl UpdateItem for PaprikaMenuItem {
    async fn existing_items(pool: &sqlx::Pool<sqlx::Postgres>) -> anyhow::Result<Vec<Self>> {
        let menu_items = sqlx::query!("SELECT data FROM menu_item")
            .map(|row| serde_json::from_value(row.data).unwrap())
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();
        Ok(menu_items)
    }

    async fn current_items(paprika: &PaprikaClient) -> anyhow::Result<Vec<Self>> {
        let menu_items = paprika.menu_items().await?;
        Ok(menu_items)
    }

    async fn on_add(
        _paprika: &PaprikaClient,
        pool: &sqlx::Pool<sqlx::Postgres>,
        new_item: &Self,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            "INSERT INTO menu_item (uid, data) VALUES ($1, $2)",
            new_item.uid,
            serde_json::to_value(&new_item)?
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
            "UPDATE menu_item SET data = $2 WHERE uid = $1",
            new_item.uid,
            serde_json::to_value(&new_item)?
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
        sqlx::query!("DELETE FROM menu_item WHERE uid = $1", old_item.uid)
            .execute(pool)
            .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl UpdateItem for PaprikaCategory {
    async fn existing_items(pool: &sqlx::Pool<sqlx::Postgres>) -> anyhow::Result<Vec<Self>> {
        let categories = sqlx::query!("SELECT data FROM category")
            .map(|row| serde_json::from_value(row.data).unwrap())
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();
        Ok(categories)
    }

    async fn current_items(paprika: &PaprikaClient) -> anyhow::Result<Vec<Self>> {
        let categories = paprika.categories().await?;
        Ok(categories)
    }

    async fn on_add(
        _paprika: &PaprikaClient,
        pool: &sqlx::Pool<sqlx::Postgres>,
        new_item: &Self,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            "INSERT INTO category (uid, data) VALUES ($1, $2)",
            new_item.uid,
            serde_json::to_value(&new_item)?
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
            "UPDATE category SET data = $2 WHERE uid = $1",
            new_item.uid,
            serde_json::to_value(&new_item)?
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
        sqlx::query!("DELETE FROM category WHERE uid = $1", old_item.uid)
            .execute(pool)
            .await?;
        Ok(())
    }
}
