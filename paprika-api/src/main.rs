use std::sync::Arc;

use actix_cors::Cors;
use actix_web::{http::header, web, App, Error, HttpRequest, HttpResponse, HttpServer};
use dataloader::{cached::Loader, BatchFn};
use juniper::{
    graphql_object, graphql_value, EmptySubscription, FieldError, GraphQLObject, RootNode,
};
use juniper_actix::{graphiql_handler, graphql_handler, playground_handler};
use paprika_client::PaprikaClient;
use updates::State;

mod updates;

#[actix_web::main]
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

    let paprika = Arc::new(paprika);

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(Connections {
                pool: pool.clone(),
                paprika: paprika.clone(),
            }))
            .app_data(web::Data::new(Schema::new(
                Query,
                Mutation,
                Default::default(),
            )))
            .wrap(
                Cors::default()
                    .allow_any_origin()
                    .allowed_methods(vec!["POST", "GET"])
                    .allowed_header(header::CONTENT_TYPE)
                    .max_age(3600),
            )
            .service(
                web::resource("/graphql")
                    .route(web::post().to(graphql_route))
                    .route(web::get().to(graphql_route)),
            )
            .service(web::resource("/playground").route(web::get().to(playground_route)))
            .service(web::resource("/graphiql").route(web::get().to(graphiql_route)))
    })
    .bind("0.0.0.0:8080")
    .unwrap()
    .run()
    .await
    .unwrap();
}

#[derive(Clone, Copy, Debug)]
struct DbError;

#[derive(Clone)]
struct Connections {
    pool: sqlx::Pool<sqlx::Postgres>,
    paprika: Arc<PaprikaClient>,
}

#[derive(Clone)]
struct Context {
    conns: Arc<Connections>,

    recipe_loader: Loader<String, Result<Recipe, DbError>, RecipeBatcher>,
    aisle_loader: Loader<String, Result<Aisle, DbError>, AisleBatcher>,
    meal_type_loader: Loader<String, Result<MealType, DbError>, MealTypeBatcher>,
    grocery_list_loader: Loader<String, Result<GroceryList, DbError>, GroceryListBatcher>,
    menu_loader: Loader<String, Result<Menu, DbError>, MenuBatcher>,
    category_loader: Loader<String, Result<Category, DbError>, CategoryBatcher>,
}

impl juniper::Context for Context {}

#[derive(Debug, Clone)]
struct Recipe {
    id: i32,
    uid: String,

    name: String,

    cook_time: Option<String>,
    prep_time: Option<String>,
    total_time: Option<String>,

    description: Option<String>,
    directions: String,
    ingredients: String,
    notes: String,

    categories: Vec<String>,
}

impl Recipe {
    async fn all(context: &Context) -> Result<Vec<Recipe>, FieldError> {
        let recipes = sqlx::query_as!(
            Recipe,
            r#"SELECT
                id,
                uid,
                name,
                cook_time,
                prep_time,
                total_time,
                description,
                directions,
                ingredients,
                notes,
                array(select category_uid from recipe_category where recipe_category.recipe_uid = recipe.uid) "categories!"
            FROM
                recipe"#
        )
        .fetch_all(&context.conns.pool)
        .await
        .map_err(|err| {
            tracing::error!("recipe fetch error: {:?}", err);
            FieldError::new("could not query database", graphql_value!(None))
        })?;

        Ok(recipes)
    }

    async fn from_id(context: &Context, id: i32) -> Result<Option<Recipe>, FieldError> {
        sqlx::query_as!(
            Recipe,
            r#"SELECT
                id,
                uid,
                name,
                cook_time,
                prep_time,
                total_time,
                description,
                directions,
                ingredients,
                notes,
                array(select category_uid from recipe_category where recipe_category.recipe_uid = recipe.uid) "categories!"
            FROM
                recipe
            WHERE
                id = $1"#,
            id
        )
        .fetch_optional(&context.conns.pool)
        .await
        .map_err(|_err| FieldError::new("could not query database", graphql_value!(None)))
    }

    async fn in_category(context: &Context, category_uid: &str) -> Result<Vec<Self>, FieldError> {
        sqlx::query_as!(
            Self,
            r#"SELECT
                id,
                uid,
                name,
                cook_time,
                prep_time,
                total_time,
                description,
                directions,
                ingredients,
                notes,
                array(select category_uid from recipe_category where recipe_category.recipe_uid = recipe.uid) "categories!"
            FROM
                recipe
            JOIN recipe_category
                ON recipe.uid = recipe_category.recipe_uid
            WHERE
                recipe_category.category_uid = $1"#,
            category_uid
        )
        .fetch_all(&context.conns.pool)
        .await
        .map_err(|_err| FieldError::new("could not query database", graphql_value!(None)))
    }
}

#[graphql_object(context = Context)]
impl Recipe {
    fn id(&self) -> i32 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn cook_time(&self) -> Option<&str> {
        self.cook_time.as_deref().filter(|s| !s.trim().is_empty())
    }

    fn prep_time(&self) -> Option<&str> {
        self.prep_time.as_deref().filter(|s| !s.trim().is_empty())
    }

    fn total_time(&self) -> Option<&str> {
        self.total_time.as_deref().filter(|s| !s.trim().is_empty())
    }

    fn description(&self) -> Option<&str> {
        self.description.as_deref().filter(|s| !s.trim().is_empty())
    }

    fn directions(&self) -> &str {
        &self.directions
    }

    fn ingredients(&self) -> &str {
        &self.ingredients
    }

    fn notes(&self) -> Option<&str> {
        if self.notes.trim().is_empty() {
            None
        } else {
            Some(self.notes.as_str())
        }
    }

    async fn meals(&self, context: &Context) -> Result<Vec<Meal>, FieldError> {
        Meal::by_recipe_uid(context, &self.uid).await
    }

    async fn categories(&self, context: &Context) -> Result<Vec<Category>, FieldError> {
        context
            .category_loader
            .load_many(self.categories.clone())
            .await
            .into_iter()
            .map(|(_uid, category)| {
                category.map_err(|_err| {
                    FieldError::new("item should always have category", graphql_value!(None))
                })
            })
            .collect()
    }

    async fn photos(&self, context: &Context) -> Result<Vec<Photo>, FieldError> {
        Photo::by_recipe_uid(context, &self.uid).await
    }
}

struct RecipeBatcher(sqlx::Pool<sqlx::Postgres>);

#[async_trait::async_trait]
impl BatchFn<String, Result<Recipe, DbError>> for RecipeBatcher {
    async fn load(
        &mut self,
        keys: &[String],
    ) -> std::collections::HashMap<String, Result<Recipe, DbError>> {
        let recipes = sqlx::query_as!(
            Recipe,
            "SELECT
                id,
                uid,
                name,
                cook_time,
                prep_time,
                total_time,
                description,
                directions,
                ingredients,
                notes,
                categories
            FROM
                recipe
            WHERE uid = any($1)",
            keys
        )
        .fetch_all(&self.0)
        .await;

        match recipes {
            Ok(recipes) => recipes
                .into_iter()
                .map(|recipe| (recipe.uid.clone(), Ok(recipe)))
                .collect(),
            Err(_err) => keys.iter().map(|k| (k.to_owned(), Err(DbError))).collect(),
        }
    }
}

struct Meal {
    id: i32,
    name: String,
    date: chrono::DateTime<chrono::Utc>,

    recipe_uid: Option<String>,
    type_uid: String,
}

impl Meal {
    async fn all(context: &Context) -> Result<Vec<Self>, FieldError> {
        let meals = sqlx::query_as!(
            Meal,
            r#"SELECT id, date, name, recipe_uid, type_uid FROM meal"#
        )
        .fetch_all(&context.conns.pool)
        .await
        .map_err(|_err| FieldError::new("could not query database", graphql_value!(None)))?;

        Ok(meals)
    }

    async fn by_recipe_uid(context: &Context, recipe_uid: &str) -> Result<Vec<Self>, FieldError> {
        let meals = sqlx::query_as!(
            Meal,
            r#"SELECT id, date, name, recipe_uid, type_uid FROM meal WHERE recipe_uid = $1"#,
            recipe_uid
        )
        .fetch_all(&context.conns.pool)
        .await
        .map_err(|_err| FieldError::new("could not query database", graphql_value!(None)))?;

        Ok(meals)
    }
}

#[graphql_object(context = Context)]
impl Meal {
    fn id(&self) -> i32 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn date(&self) -> chrono::DateTime<chrono::Utc> {
        self.date
    }

    async fn recipe(&self, context: &Context) -> Result<Option<Recipe>, FieldError> {
        let recipe_uid = match &self.recipe_uid {
            Some(recipe_uid) => recipe_uid.clone(),
            None => return Ok(None),
        };

        context
            .recipe_loader
            .load(recipe_uid)
            .await
            .map_err(|_err| FieldError::new("item should always have recipe", graphql_value!(None)))
            .map(Option::Some)
    }

    async fn meal_type(&self, context: &Context) -> Result<MealType, FieldError> {
        context
            .meal_type_loader
            .load(self.type_uid.clone())
            .await
            .map_err(|_err| {
                FieldError::new("item should always have meal type", graphql_value!(None))
            })
    }
}

struct GroceryItem {
    id: i32,

    name: String,
    ingredient: String,
    quantity: String,
    instruction: String,

    purchased: bool,
    aisle_uid: String,
    list_uid: String,

    recipe: Option<String>,
}

impl GroceryItem {
    async fn all(context: &Context) -> Result<Vec<Self>, FieldError> {
        sqlx::query_as!(
            GroceryItem,
            r#"SELECT
                id,
                name,
                ingredient,
                quantity,
                instruction,
                purchased,
                aisle_uid,
                list_uid,
                recipe
            FROM
                grocery_item"#
        )
        .fetch_all(&context.conns.pool)
        .await
        .map_err(|_err| FieldError::new("could not query database", graphql_value!(None)))
    }

    async fn by_list_uid(context: &Context, list_uid: &str) -> Result<Vec<Self>, FieldError> {
        sqlx::query_as!(
            GroceryItem,
            r#"SELECT
                id,
                name,
                ingredient,
                quantity,
                instruction,
                purchased,
                aisle_uid,
                list_uid,
                recipe
            FROM
                grocery_item
            WHERE
                list_uid = $1"#,
            list_uid
        )
        .fetch_all(&context.conns.pool)
        .await
        .map_err(|_err| FieldError::new("could not query database", graphql_value!(None)))
    }
}

#[graphql_object(context = Context)]
impl GroceryItem {
    fn id(&self) -> i32 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn ingredient(&self) -> &str {
        &self.ingredient
    }

    fn quantity(&self) -> &str {
        &self.quantity
    }

    fn instruction(&self) -> &str {
        &self.instruction
    }

    fn purchased(&self) -> bool {
        self.purchased
    }

    fn recipe_name(&self) -> Option<&str> {
        self.recipe.as_deref()
    }

    async fn aisle(&self, context: &Context) -> Result<Aisle, FieldError> {
        context
            .aisle_loader
            .load(self.aisle_uid.clone())
            .await
            .map_err(|_err| FieldError::new("item should always have aisle", graphql_value!(None)))
    }

    async fn list(&self, context: &Context) -> Result<GroceryList, FieldError> {
        context
            .grocery_list_loader
            .load(self.list_uid.clone())
            .await
            .map_err(|_err| {
                FieldError::new("item should always have grocery list", graphql_value!(None))
            })
    }
}

#[derive(GraphQLObject, Debug, Clone)]
struct Aisle {
    id: i32,
    #[graphql(skip)]
    uid: String,
    name: String,
    order_flag: i32,
}

struct AisleBatcher(sqlx::Pool<sqlx::Postgres>);

#[async_trait::async_trait]
impl BatchFn<String, Result<Aisle, DbError>> for AisleBatcher {
    async fn load(
        &mut self,
        keys: &[String],
    ) -> std::collections::HashMap<String, Result<Aisle, DbError>> {
        let aisles = sqlx::query_as!(
            Aisle,
            "SELECT
                id,
                uid,
                name,
                order_flag
            FROM
                aisle
            WHERE uid = any($1)",
            keys
        )
        .fetch_all(&self.0)
        .await;

        match aisles {
            Ok(aisles) => aisles
                .into_iter()
                .map(|aisle| (aisle.uid.clone(), Ok(aisle)))
                .collect(),
            Err(_err) => keys.iter().map(|k| (k.to_owned(), Err(DbError))).collect(),
        }
    }
}

struct PantryItem {
    id: i32,
    ingredient: String,
    expiration_date: Option<chrono::DateTime<chrono::Utc>>,
    in_stock: bool,
    purchase_date: chrono::DateTime<chrono::Utc>,
    quantity: String,
    aisle_uid: String,
}

impl PantryItem {
    async fn all(context: &Context) -> Result<Vec<PantryItem>, FieldError> {
        sqlx::query_as!(
            PantryItem,
            r#"SELECT
                id,
                ingredient,
                expiration_date,
                in_stock,
                purchase_date,
                quantity,
                aisle_uid
            FROM
                pantry_item"#
        )
        .fetch_all(&context.conns.pool)
        .await
        .map_err(|_err| FieldError::new("could not query database", graphql_value!(None)))
    }
}

#[graphql_object(context = Context)]
impl PantryItem {
    fn id(&self) -> i32 {
        self.id
    }

    fn ingredient(&self) -> &str {
        &self.ingredient
    }

    fn expiration_date(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.expiration_date
    }

    fn in_stock(&self) -> bool {
        self.in_stock
    }

    fn purchase_date(&self) -> chrono::DateTime<chrono::Utc> {
        self.purchase_date
    }

    fn quantity(&self) -> &str {
        &self.quantity
    }

    async fn aisle(&self, context: &Context) -> Result<Aisle, FieldError> {
        context
            .aisle_loader
            .load(self.aisle_uid.clone())
            .await
            .map_err(|_err| FieldError::new("item should always have aisle", graphql_value!(None)))
    }
}

#[derive(Debug, Clone)]
struct MealType {
    id: i32,
    uid: String,
    name: String,
}

#[graphql_object(context = Context)]
impl MealType {
    fn id(&self) -> i32 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }
}

struct MealTypeBatcher(sqlx::Pool<sqlx::Postgres>);

#[async_trait::async_trait]
impl BatchFn<String, Result<MealType, DbError>> for MealTypeBatcher {
    async fn load(
        &mut self,
        keys: &[String],
    ) -> std::collections::HashMap<String, Result<MealType, DbError>> {
        let meal_types = sqlx::query_as!(
            MealType,
            "SELECT id, uid, name FROM meal_type WHERE uid = any($1)",
            keys
        )
        .fetch_all(&self.0)
        .await;

        match meal_types {
            Ok(meal_types) => meal_types
                .into_iter()
                .map(|meal_type| (meal_type.uid.clone(), Ok(meal_type)))
                .collect(),
            Err(_err) => keys.iter().map(|k| (k.to_owned(), Err(DbError))).collect(),
        }
    }
}

struct MenuItem {
    id: i32,
    name: String,
    recipe_uid: String,
    menu_uid: String,
    type_uid: String,
    day: i32,
}

impl MenuItem {
    async fn by_menu_uid(context: &Context, menu_uid: &str) -> Result<Vec<Self>, FieldError> {
        sqlx::query_as!(
            MenuItem,
            "SELECT id, name, recipe_uid, menu_uid, type_uid, day FROM menu_item WHERE menu_uid = $1",
            menu_uid
        )
        .fetch_all(&context.conns.pool)
        .await
        .map_err(|_err| FieldError::new("could not query database", graphql_value!(None)))
    }
}

#[graphql_object(context = Context)]
impl MenuItem {
    fn id(&self) -> i32 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn day(&self) -> i32 {
        self.day
    }

    async fn menu(&self, context: &Context) -> Result<Menu, FieldError> {
        context
            .menu_loader
            .load(self.menu_uid.clone())
            .await
            .map_err(|_err| FieldError::new("item should always have menu", graphql_value!(None)))
    }

    async fn recipe(&self, context: &Context) -> Result<Recipe, FieldError> {
        context
            .recipe_loader
            .load(self.recipe_uid.clone())
            .await
            .map_err(|_err| FieldError::new("item should always have recipe", graphql_value!(None)))
    }

    async fn meal_type(&self, context: &Context) -> Result<MealType, FieldError> {
        context
            .meal_type_loader
            .load(self.type_uid.clone())
            .await
            .map_err(|_err| {
                FieldError::new("item should always have meal type", graphql_value!(None))
            })
    }
}

#[derive(Clone, Debug)]
struct Menu {
    id: i32,
    uid: String,
    name: String,
    notes: String,
    days: i32,
}

impl Menu {
    async fn all(context: &Context) -> Result<Vec<Self>, FieldError> {
        sqlx::query_as!(Self, "SELECT id, uid, name, notes, days FROM menu")
            .fetch_all(&context.conns.pool)
            .await
            .map_err(|_err| FieldError::new("could not query database", graphql_value!(None)))
    }
}

#[graphql_object(context = Context)]
impl Menu {
    fn id(&self) -> i32 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn notes(&self) -> &str {
        &self.notes
    }

    fn days(&self) -> i32 {
        self.days
    }

    async fn items(&self, context: &Context) -> Result<Vec<MenuItem>, FieldError> {
        MenuItem::by_menu_uid(context, &self.uid).await
    }
}

struct MenuBatcher(sqlx::Pool<sqlx::Postgres>);

#[async_trait::async_trait]
impl BatchFn<String, Result<Menu, DbError>> for MenuBatcher {
    async fn load(
        &mut self,
        keys: &[String],
    ) -> std::collections::HashMap<String, Result<Menu, DbError>> {
        let menus = sqlx::query_as!(
            Menu,
            "SELECT id, uid, name, notes, days FROM menu WHERE uid = any($1)",
            keys
        )
        .fetch_all(&self.0)
        .await;

        match menus {
            Ok(menus) => menus
                .into_iter()
                .map(|menu| (menu.uid.clone(), Ok(menu)))
                .collect(),
            Err(_err) => keys.iter().map(|k| (k.to_owned(), Err(DbError))).collect(),
        }
    }
}

struct Bookmark {
    id: i32,
    title: String,
    url: String,
}

impl Bookmark {
    async fn all(context: &Context) -> Result<Vec<Self>, FieldError> {
        sqlx::query_as!(Self, "SELECT id, title, url FROM bookmark")
            .fetch_all(&context.conns.pool)
            .await
            .map_err(|_err| FieldError::new("could not query database", graphql_value!(None)))
    }
}

#[graphql_object(context = Context)]
impl Bookmark {
    fn id(&self) -> i32 {
        self.id
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn url(&self) -> &str {
        &self.url
    }
}

#[derive(Clone, Debug)]
struct Category {
    id: i32,
    uid: String,
    name: String,
    parent_uid: Option<String>,
}

impl Category {
    async fn all(context: &Context) -> Result<Vec<Self>, FieldError> {
        sqlx::query_as!(Self, r"SELECT id, uid, name, parent_uid FROM category")
            .fetch_all(&context.conns.pool)
            .await
            .map_err(|_err| FieldError::new("could not query database", graphql_value!(None)))
    }
}

#[graphql_object(context = Context)]
impl Category {
    fn id(&self) -> i32 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    async fn parent(&self, context: &Context) -> Result<Option<Self>, FieldError> {
        if let Some(parent_uid) = &self.parent_uid {
            context
                .category_loader
                .load(parent_uid.clone())
                .await
                .map(Some)
                .map_err(|_err| {
                    FieldError::new("item should always have parent", graphql_value!(None))
                })
        } else {
            Ok(None)
        }
    }

    async fn recipes(&self, context: &Context) -> Result<Vec<Recipe>, FieldError> {
        Recipe::in_category(context, &self.uid).await
    }
}

struct CategoryBatcher(sqlx::Pool<sqlx::Postgres>);

#[async_trait::async_trait]
impl BatchFn<String, Result<Category, DbError>> for CategoryBatcher {
    async fn load(
        &mut self,
        keys: &[String],
    ) -> std::collections::HashMap<String, Result<Category, DbError>> {
        let categories = sqlx::query_as!(
            Category,
            r"SELECT id, uid, name, parent_uid FROM category WHERE uid = any($1)",
            keys
        )
        .fetch_all(&self.0)
        .await;

        match categories {
            Ok(categories) => categories
                .into_iter()
                .map(|category| (category.uid.clone(), Ok(category)))
                .collect(),
            Err(_err) => keys.iter().map(|k| (k.to_owned(), Err(DbError))).collect(),
        }
    }
}

struct Photo {
    id: i32,
    filename: String,
    recipe_uid: String,
    hash: String,
}

impl Photo {
    async fn all(context: &Context) -> Result<Vec<Self>, FieldError> {
        sqlx::query_as!(Self, r"SELECT id, filename, recipe_uid, hash FROM photo")
            .fetch_all(&context.conns.pool)
            .await
            .map_err(|_err| FieldError::new("could not query database", graphql_value!(None)))
    }

    async fn by_recipe_uid(context: &Context, recipe_uid: &str) -> Result<Vec<Self>, FieldError> {
        sqlx::query_as!(
            Self,
            r"SELECT id, filename, recipe_uid, hash FROM photo WHERE recipe_uid = $1",
            recipe_uid
        )
        .fetch_all(&context.conns.pool)
        .await
        .map_err(|_err| FieldError::new("could not query database", graphql_value!(None)))
    }
}

#[graphql_object(context = Context)]
impl Photo {
    fn id(&self) -> i32 {
        self.id
    }

    fn filename(&self) -> &str {
        &self.filename
    }

    fn hash(&self) -> &str {
        &self.hash
    }

    async fn recipe(&self, context: &Context) -> Result<Recipe, FieldError> {
        context
            .recipe_loader
            .load(self.recipe_uid.clone())
            .await
            .map_err(|_err| FieldError::new("item should always have recipe", graphql_value!(None)))
    }
}

#[derive(Debug, Clone)]
struct GroceryList {
    id: i32,
    uid: String,
    name: String,
    is_default: bool,
}

impl GroceryList {
    async fn all(context: &Context) -> Result<Vec<Self>, FieldError> {
        sqlx::query_as!(Self, r"SELECT id, uid, name, is_default FROM grocery_list")
            .fetch_all(&context.conns.pool)
            .await
            .map_err(|_err| FieldError::new("could not query database", graphql_value!(None)))
    }
}

#[graphql_object(context = Context)]
impl GroceryList {
    fn id(&self) -> i32 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn is_default(&self) -> bool {
        self.is_default
    }

    async fn items(&self, context: &Context) -> Result<Vec<GroceryItem>, FieldError> {
        GroceryItem::by_list_uid(context, &self.uid).await
    }
}

struct GroceryListBatcher(sqlx::Pool<sqlx::Postgres>);

#[async_trait::async_trait]
impl BatchFn<String, Result<GroceryList, DbError>> for GroceryListBatcher {
    async fn load(
        &mut self,
        keys: &[String],
    ) -> std::collections::HashMap<String, Result<GroceryList, DbError>> {
        let grocery_lists = sqlx::query_as!(
            GroceryList,
            "SELECT id, uid, name, is_default FROM grocery_list WHERE uid = any($1)",
            keys
        )
        .fetch_all(&self.0)
        .await;

        match grocery_lists {
            Ok(grocery_lists) => grocery_lists
                .into_iter()
                .map(|grocery_list| (grocery_list.uid.clone(), Ok(grocery_list)))
                .collect(),
            Err(_err) => keys.iter().map(|k| (k.to_owned(), Err(DbError))).collect(),
        }
    }
}

struct GroceryIngredient {
    id: i32,
    name: String,
    aisle_uid: Option<String>,
}

impl GroceryIngredient {
    async fn all(context: &Context) -> Result<Vec<Self>, FieldError> {
        sqlx::query_as!(Self, r"SELECT id, name, aisle_uid FROM grocery_ingredient")
            .fetch_all(&context.conns.pool)
            .await
            .map_err(|_err| FieldError::new("could not query database", graphql_value!(None)))
    }
}

#[graphql_object(context = Context)]
impl GroceryIngredient {
    fn id(&self) -> i32 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    async fn aisle(&self, context: &Context) -> Result<Option<Aisle>, FieldError> {
        if let Some(aisle_uid) = &self.aisle_uid {
            context
                .aisle_loader
                .load(aisle_uid.clone())
                .await
                .map(Some)
                .map_err(|_err| {
                    FieldError::new("item should always have aisle", graphql_value!(None))
                })
        } else {
            Ok(None)
        }
    }
}

struct Query;

#[graphql_object(context = Context)]
impl Query {
    async fn recipe(context: &Context, id: i32) -> Result<Option<Recipe>, FieldError> {
        Recipe::from_id(context, id).await
    }

    async fn recipes(context: &Context) -> Result<Vec<Recipe>, FieldError> {
        Recipe::all(context).await
    }

    async fn meals(context: &Context) -> Result<Vec<Meal>, FieldError> {
        Meal::all(context).await
    }

    async fn groceries(context: &Context) -> Result<Vec<GroceryItem>, FieldError> {
        GroceryItem::all(context).await
    }

    async fn pantry_items(context: &Context) -> Result<Vec<PantryItem>, FieldError> {
        PantryItem::all(context).await
    }

    async fn menus(context: &Context) -> Result<Vec<Menu>, FieldError> {
        Menu::all(context).await
    }

    async fn bookmarks(context: &Context) -> Result<Vec<Bookmark>, FieldError> {
        Bookmark::all(context).await
    }

    async fn categories(context: &Context) -> Result<Vec<Category>, FieldError> {
        Category::all(context).await
    }

    async fn photos(context: &Context) -> Result<Vec<Photo>, FieldError> {
        Photo::all(context).await
    }

    async fn grocery_lists(context: &Context) -> Result<Vec<GroceryList>, FieldError> {
        GroceryList::all(context).await
    }

    async fn grocery_ingredients(context: &Context) -> Result<Vec<GroceryIngredient>, FieldError> {
        GroceryIngredient::all(context).await
    }
}

struct Mutation;

#[graphql_object(context = Context)]
impl Mutation {
    async fn sync(context: &Context) -> Result<bool, FieldError> {
        let changes =
            updates::check_for_updates(&context.conns.paprika, &context.conns.pool).await?;
        let had_changes = changes.contains_key(&State::Added)
            || changes.contains_key(&State::Deleted)
            || changes.contains_key(&State::Changed);

        Ok(had_changes)
    }
}

type Schema = RootNode<'static, Query, Mutation, EmptySubscription<Context>>;

async fn graphiql_route() -> Result<HttpResponse, Error> {
    graphiql_handler("/graphql", None).await
}

async fn playground_route() -> Result<HttpResponse, Error> {
    playground_handler("/graphql", None).await
}

async fn graphql_route(
    req: HttpRequest,
    payload: web::Payload,
    schema: web::Data<Schema>,
    conns: web::Data<Connections>,
) -> Result<HttpResponse, Error> {
    let context = Context {
        recipe_loader: Loader::new(RecipeBatcher(conns.pool.clone())),
        aisle_loader: Loader::new(AisleBatcher(conns.pool.clone())),
        meal_type_loader: Loader::new(MealTypeBatcher(conns.pool.clone())),
        grocery_list_loader: Loader::new(GroceryListBatcher(conns.pool.clone())),
        menu_loader: Loader::new(MenuBatcher(conns.pool.clone())),
        category_loader: Loader::new(CategoryBatcher(conns.pool.clone())),

        conns: (*conns).clone(),
    };

    graphql_handler(&schema, &context, req, payload).await
}
