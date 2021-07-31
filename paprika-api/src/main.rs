use std::sync::Arc;

use actix_cors::Cors;
use actix_web::{http::header, web, App, Error, HttpRequest, HttpResponse, HttpServer};
use juniper::{
    graphql_object, graphql_value, EmptyMutation, EmptySubscription, FieldError, GraphQLObject,
    RootNode,
};
use juniper_actix::{graphiql_handler, graphql_handler, playground_handler};
use paprika_client::PaprikaClient;

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

    updates::check_for_updates(&paprika, &pool).await.unwrap();
    tracing::info!("completed database update");

    let paprika = Arc::new(paprika);

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(Context {
                pool: pool.clone(),
                paprika: paprika.clone(),
            }))
            .app_data(web::Data::new(Schema::new(
                Query,
                Default::default(),
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

#[derive(Clone)]
struct Context {
    pool: sqlx::Pool<sqlx::Postgres>,
    paprika: Arc<PaprikaClient>,
}

impl juniper::Context for Context {}

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
                notes
            FROM
                recipe"#
        )
        .fetch_all(&context.pool)
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
                notes
            FROM
                recipe
            WHERE
                id = $1"#,
            id
        )
        .fetch_optional(&context.pool)
        .await
        .map_err(|_err| FieldError::new("could not query database", graphql_value!(None)))
    }

    async fn from_uid(context: &Context, uid: &str) -> Result<Option<Recipe>, FieldError> {
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
                notes
            FROM
                recipe
            WHERE
                uid = $1"#,
            uid
        )
        .fetch_optional(&context.pool)
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
        let meals = sqlx::query_as!(
            Meal,
            r#"SELECT id, date, name, recipe_uid FROM meal WHERE recipe_uid = $1"#,
            self.uid
        )
        .fetch_all(&context.pool)
        .await
        .map_err(|_err| FieldError::new("could not query database", graphql_value!(None)))?;

        Ok(meals)
    }
}

struct Meal {
    id: i32,
    name: String,
    date: chrono::DateTime<chrono::Utc>,

    recipe_uid: String,
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
        Recipe::from_uid(&context, &self.recipe_uid).await
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

    recipe_uid: Option<String>,
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

    async fn recipe(&self, context: &Context) -> Result<Option<Recipe>, FieldError> {
        let recipe_uid = match &self.recipe_uid {
            Some(recipe_uid) => recipe_uid,
            None => return Ok(None),
        };

        Recipe::from_uid(&context, &recipe_uid).await
    }

    async fn aisle(&self, context: &Context) -> Result<Aisle, FieldError> {
        let aisle = Aisle::from_uid(&context, &self.aisle_uid).await?;
        aisle.ok_or_else(|| FieldError::new("item should always have aisle", graphql_value!(None)))
    }
}

#[derive(GraphQLObject)]
struct Aisle {
    id: i32,
    name: String,
    order_flag: i32,
}

impl Aisle {
    async fn from_uid(context: &Context, uid: &str) -> Result<Option<Aisle>, FieldError> {
        sqlx::query_as!(
            Aisle,
            r#"SELECT
                id,
                name,
                order_flag
            FROM
                aisle
            WHERE
                uid = $1"#,
            uid
        )
        .fetch_optional(&context.pool)
        .await
        .map_err(|_err| FieldError::new("could not query database", graphql_value!(None)))
    }
}

struct Query;

#[graphql_object(context = Context)]
impl Query {
    async fn recipe(context: &Context, id: i32) -> Result<Option<Recipe>, FieldError> {
        Recipe::from_id(&context, id).await
    }

    async fn recipes(context: &Context) -> Result<Vec<Recipe>, FieldError> {
        Recipe::all(&context).await
    }

    async fn meal(context: &Context, id: i32) -> Result<Option<Meal>, FieldError> {
        let meal = sqlx::query_as!(
            Meal,
            r#"SELECT id, date, name, recipe_uid FROM meal WHERE id = $1"#,
            id
        )
        .fetch_optional(&context.pool)
        .await
        .map_err(|_err| FieldError::new("could not query database", graphql_value!(None)))?;

        Ok(meal)
    }

    async fn meals(context: &Context) -> Result<Vec<Meal>, FieldError> {
        let meals = sqlx::query_as!(Meal, r#"SELECT id, date, name, recipe_uid FROM meal"#)
            .fetch_all(&context.pool)
            .await
            .map_err(|_err| FieldError::new("could not query database", graphql_value!(None)))?;

        Ok(meals)
    }

    async fn groceries(context: &Context) -> Result<Vec<GroceryItem>, FieldError> {
        let groceries = sqlx::query_as!(
            GroceryItem,
            r#"SELECT
                id,
                name,
                ingredient,
                quantity,
                instruction,
                purchased,
                aisle_uid,
                recipe_uid
            FROM
                grocery_item"#
        )
        .fetch_all(&context.pool)
        .await
        .map_err(|_err| FieldError::new("could not query database", graphql_value!(None)))?;

        Ok(groceries)
    }
}

type Schema = RootNode<'static, Query, EmptyMutation<Context>, EmptySubscription<Context>>;

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
    context: web::Data<Context>,
) -> Result<HttpResponse, Error> {
    graphql_handler(&schema, &context, req, payload).await
}
