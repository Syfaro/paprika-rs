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
            "INSERT INTO recipe (categories, cook_time, created, description, difficulty, directions, hash, image_url, in_trash, ingredients, is_pinned, name, notes, on_favorites, on_grocery_list, photo, photo_hash, photo_large, photo_url, prep_time, rating, scale, servings, source, source_url, total_time, uid)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27)",
            &recipe.categories,
            recipe.cook_time,
            recipe.created,
            recipe.description,
            recipe.difficulty,
            recipe.directions,
            recipe.hash,
            recipe.image_url,
            recipe.in_trash,
            recipe.ingredients,
            recipe.is_pinned,
            recipe.name,
            recipe.notes,
            recipe.on_favorites,
            recipe.on_grocery_list,
            recipe.photo,
            recipe.photo_hash,
            recipe.photo_large,
            recipe.photo_url,
            recipe.prep_time,
            recipe.rating,
            recipe.scale,
            recipe.servings,
            recipe.source,
            recipe.source_url,
            recipe.total_time,
            recipe.uid,
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
            "UPDATE recipe SET
                categories = $2,
                cook_time = $3,
                created = $4,
                description = $5,
                difficulty = $6,
                directions = $7,
                hash = $8,
                image_url = $9,
                in_trash = $10,
                ingredients = $11,
                is_pinned = $12,
                name = $13,
                notes = $14,
                on_favorites = $15,
                on_grocery_list = $16,
                photo = $17,
                photo_hash = $18,
                photo_large = $19,
                photo_url = $20,
                prep_time = $21,
                rating = $22,
                scale = $23,
                servings = $24,
                source = $25,
                source_url = $26,
                total_time = $27
            WHERE uid = $1",
            recipe.uid,
            &recipe.categories,
            recipe.cook_time,
            recipe.created,
            recipe.description,
            recipe.difficulty,
            recipe.directions,
            recipe.hash,
            recipe.image_url,
            recipe.in_trash,
            recipe.ingredients,
            recipe.is_pinned,
            recipe.name,
            recipe.notes,
            recipe.on_favorites,
            recipe.on_grocery_list,
            recipe.photo,
            recipe.photo_hash,
            recipe.photo_large,
            recipe.photo_url,
            recipe.prep_time,
            recipe.rating,
            recipe.scale,
            recipe.servings,
            recipe.source,
            recipe.source_url,
            recipe.total_time
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
        let meals = sqlx::query_as!(
            Self,
            "SELECT uid, recipe_uid, date, meal_type, name, order_flag, type_uid FROM meal"
        )
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
            "INSERT INTO meal (uid, recipe_uid, date, meal_type, name, order_flag, type_uid) VALUES ($1, $2, $3, $4, $5, $6, $7)",
            new_item.uid,
            new_item.recipe_uid,
            new_item.date,
            new_item.meal_type,
            new_item.name,
            new_item.order_flag,
            new_item.type_uid
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
            "UPDATE meal SET recipe_uid = $2, date = $3, meal_type = $4, name = $5, order_flag = $6, type_uid = $7 WHERE uid = $1",
            new_item.uid,
            new_item.recipe_uid,
            new_item.date,
            new_item.meal_type,
            new_item.name,
            new_item.order_flag,
            new_item.type_uid
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
        let groceries = sqlx::query_as!(Self, "SELECT uid, recipe_uid, name, order_flag, purchased, aisle, ingredient, recipe, instruction, quantity, separate, aisle_uid, list_uid FROM grocery_item")
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
            "INSERT INTO grocery_item (uid, recipe_uid, name, order_flag, purchased, aisle, ingredient, recipe, instruction, quantity, separate, aisle_uid, list_uid) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
            new_item.uid,
            new_item.recipe_uid,
            new_item.name,
            new_item.order_flag,
            new_item.purchased,
            new_item.aisle,
            new_item.ingredient,
            new_item.recipe,
            new_item.instruction,
            new_item.quantity,
            new_item.separate,
            new_item.aisle_uid,
            new_item.list_uid
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
            "UPDATE grocery_item SET recipe_uid = $2, name = $3, order_flag = $4, purchased = $5, aisle = $6, ingredient = $7, recipe = $8, instruction = $9, quantity = $10, separate = $11, aisle_uid = $12, list_uid = $13 WHERE uid = $1",
            new_item.uid,
            new_item.recipe_uid,
            new_item.name,
            new_item.order_flag,
            new_item.purchased,
            new_item.aisle,
            new_item.ingredient,
            new_item.recipe,
            new_item.instruction,
            new_item.quantity,
            new_item.separate,
            new_item.aisle_uid,
            new_item.list_uid
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
        let aisles = sqlx::query_as!(Self, "SELECT uid, name, order_flag FROM aisle")
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
            "INSERT INTO aisle (uid, name, order_flag) VALUES ($1, $2, $3)",
            new_item.uid,
            new_item.name,
            new_item.order_flag
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
            "UPDATE aisle SET name = $2, order_flag = $3 WHERE uid = $1",
            new_item.uid,
            new_item.name,
            new_item.order_flag
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
        let menus = sqlx::query_as!(Self, "SELECT uid, name, notes, order_flag, days FROM menu")
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
            "INSERT INTO menu (uid, name, notes, order_flag, days) VALUES ($1, $2, $3, $4, $5)",
            new_item.uid,
            new_item.name,
            new_item.notes,
            new_item.order_flag,
            new_item.days
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
            "UPDATE menu SET name = $2, notes = $3, order_flag = $4, days = $5 WHERE uid = $1",
            new_item.uid,
            new_item.name,
            new_item.notes,
            new_item.order_flag,
            new_item.days
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
        let photos = sqlx::query_as!(
            Self,
            "SELECT uid, filename, recipe_uid, order_flag, name, hash FROM photo"
        )
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
            "INSERT INTO photo (uid, filename, recipe_uid, order_flag, name, hash) VALUES ($1, $2, $3, $4, $5, $6)",
            new_item.uid,
            new_item.filename,
            new_item.recipe_uid,
            new_item.order_flag,
            new_item.name,
            new_item.hash
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
            "UPDATE photo SET filename = $2, recipe_uid = $3, order_flag = $4, name = $5, hash = $6 WHERE uid = $1",
            new_item.uid,
            new_item.filename,
            new_item.recipe_uid,
            new_item.order_flag,
            new_item.name,
            new_item.hash
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
        let meal_types = sqlx::query_as!(Self, "SELECT uid, name, order_flag, color, export_all_day, export_time, original_type FROM meal_type")
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
            "INSERT INTO meal_type (uid, name, order_flag, color, export_all_day, export_time, original_type) VALUES ($1, $2, $3, $4, $5, $6, $7)",
            new_item.uid,
            new_item.name,
            new_item.order_flag,
            new_item.color,
            new_item.export_all_day,
            new_item.export_time,
            new_item.original_type
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
            "UPDATE meal_type SET name = $2, order_flag = $3, color = $4, export_all_day = $5, export_time = $6, original_type = $7 WHERE uid = $1",
            new_item.uid,
            new_item.name,
            new_item.order_flag,
            new_item.color,
            new_item.export_all_day,
            new_item.export_time,
            new_item.original_type
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
        let pantry_items = sqlx::query_as!(Self, "SELECT uid, ingredient, aisle, expiration_date, has_expiration, in_stock, purchase_date, quantity, aisle_uid FROM pantry_item")
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
            "INSERT INTO pantry_item (uid, ingredient, aisle, expiration_date, has_expiration, in_stock, purchase_date, quantity, aisle_uid) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            new_item.uid,
            new_item.ingredient,
            new_item.aisle,
            new_item.expiration_date,
            new_item.has_expiration,
            new_item.in_stock,
            new_item.purchase_date,
            new_item.quantity,
            new_item.aisle_uid
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
            "UPDATE pantry_item SET ingredient = $2, aisle = $3, expiration_date = $4, has_expiration = $5, in_stock = $6, purchase_date = $7, quantity = $8, aisle_uid = $9 WHERE uid = $1",
            new_item.uid,
            new_item.ingredient,
            new_item.aisle,
            new_item.expiration_date,
            new_item.has_expiration,
            new_item.in_stock,
            new_item.purchase_date,
            new_item.quantity,
            new_item.aisle_uid
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
        let grocery_ingredients =
            sqlx::query_as!(Self, "SELECT uid, name, aisle_uid FROM grocery_ingredient")
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
            "INSERT INTO grocery_ingredient (uid, name, aisle_uid) VALUES ($1, $2, $3)",
            new_item.uid,
            new_item.name,
            new_item.aisle_uid
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
            "UPDATE grocery_ingredient SET name = $2, aisle_uid = $3 WHERE uid = $1",
            new_item.uid,
            new_item.name,
            new_item.aisle_uid
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
        let grocery_lists = sqlx::query_as!(
            Self,
            "SELECT uid, name, order_flag, is_default, reminders_list FROM grocery_list"
        )
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
            "INSERT INTO grocery_list (uid, name, order_flag, is_default, reminders_list) VALUES ($1, $2, $3, $4, $5)",
            new_item.uid,
            new_item.name,
            new_item.order_flag,
            new_item.is_default,
            new_item.reminders_list
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
            "UPDATE grocery_list SET name = $2, order_flag = $3, is_default = $4, reminders_list = $5 WHERE uid = $1",
            new_item.uid,
            new_item.name,
            new_item.order_flag,
            new_item.is_default,
            new_item.reminders_list
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
        let bookmarks = sqlx::query_as!(Self, "SELECT uid, title, url, order_flag FROM bookmark")
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
            "INSERT INTO bookmark (uid, title, url, order_flag) VALUES ($1, $2, $3, $4)",
            new_item.uid,
            new_item.title,
            new_item.url,
            new_item.order_flag
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
            "UPDATE bookmark SET title = $2, url = $3, order_flag = $4 WHERE uid = $1",
            new_item.uid,
            new_item.title,
            new_item.url,
            new_item.order_flag
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
        let menu_items = sqlx::query_as!(
            Self,
            "SELECT uid, name, order_flag, recipe_uid, menu_uid, type_uid, day FROM menu_item"
        )
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
            "INSERT INTO menu_item (uid, name, order_flag, recipe_uid, menu_uid, type_uid, day) VALUES ($1, $2, $3, $4, $5, $6, $7)",
            new_item.uid,
            new_item.name,
            new_item.order_flag,
            new_item.recipe_uid,
            new_item.menu_uid,
            new_item.type_uid,
            new_item.day
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
            "UPDATE menu_item SET name = $2, order_flag = $3, recipe_uid = $4, menu_uid = $5, type_uid = $6, day = $7 WHERE uid = $1",
            new_item.uid,
            new_item.name,
            new_item.order_flag,
            new_item.recipe_uid,
            new_item.menu_uid,
            new_item.type_uid,
            new_item.day
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
        let categories = sqlx::query_as!(
            Self,
            "SELECT uid, order_flag, name, parent_uid FROM category"
        )
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
            "INSERT INTO category (uid, order_flag, name, parent_uid) VALUES ($1, $2, $3, $4)",
            new_item.uid,
            new_item.order_flag,
            new_item.name,
            new_item.parent_uid
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
            "UPDATE category SET order_flag = $2, name = $3, parent_uid = $4 WHERE uid = $1",
            new_item.uid,
            new_item.order_flag,
            new_item.name,
            new_item.parent_uid
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
