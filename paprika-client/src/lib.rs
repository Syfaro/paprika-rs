use std::convert::{TryFrom, TryInto};

use serde::{Deserialize, Serialize};

static API_ENDPOINT: &str = "https://www.paprikaapp.com/api/v2";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
}

pub struct PaprikaClient {
    client: reqwest::Client,

    pub token: String,
}

#[derive(Deserialize)]
struct PaprikaResult<D> {
    result: D,
}

mod paprika_date_format {
    use chrono::{DateTime, TimeZone, Utc};
    use serde::{self, Deserialize, Deserializer, Serializer};

    const FORMAT: &str = "%Y-%m-%d %H:%M:%S";

    pub fn serialize<S>(date: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = date.format(FORMAT).to_string();
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Utc.datetime_from_str(&s, FORMAT)
            .map_err(serde::de::Error::custom)
    }
}

mod paprika_optional_date_format {
    use chrono::{DateTime, TimeZone, Utc};
    use serde::{self, Deserialize, Deserializer, Serializer};

    const FORMAT: &str = "%Y-%m-%d %H:%M:%S";

    pub fn serialize<S>(date: &Option<DateTime<Utc>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match date {
            Some(date) => {
                let s = date.format(FORMAT).to_string();
                serializer.serialize_str(&s)
            }
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<DateTime<Utc>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: Option<String> = Option::deserialize(deserializer)?;
        s.map(|s| {
            Utc.datetime_from_str(&s, FORMAT)
                .map_err(serde::de::Error::custom)
        })
        .transpose()
    }
}

#[derive(Deserialize)]
struct PaprikaToken {
    token: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PaprikaStatus {
    pub bookmarks: i32,
    pub categories: i32,
    pub groceries: i32,
    #[serde(rename = "groceryaisles")]
    pub grocery_aisles: i32,
    #[serde(rename = "groceryingredients")]
    pub grocery_ingredients: i32,
    #[serde(rename = "grocerylists")]
    pub grocery_lists: i32,
    pub meals: i32,
    #[serde(rename = "mealtypes")]
    pub meal_types: i32,
    #[serde(rename = "menuitems")]
    pub menu_items: i32,
    pub menus: i32,
    pub pantry: i32,
    pub photos: i32,
    pub recipes: i32,
}

impl TryInto<std::collections::HashMap<String, i32>> for PaprikaStatus {
    type Error = serde_json::Error;

    fn try_into(self) -> Result<std::collections::HashMap<String, i32>, Self::Error> {
        // A horrible hack to avoid having to hand write anything.
        let value = serde_json::to_value(self)?;
        serde_json::from_value(value)
    }
}

impl TryFrom<std::collections::HashMap<String, i32>> for PaprikaStatus {
    type Error = serde_json::Error;

    fn try_from(value: std::collections::HashMap<String, i32>) -> Result<Self, Self::Error> {
        // A horrible hack to avoid having to hand write anything.
        let value = serde_json::to_value(value)?;
        serde_json::from_value(value)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct PaprikaRecipeHash {
    pub uid: String,
    pub hash: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "database", derive(sqlx::FromRow))]
pub struct PaprikaRecipe {
    pub categories: Vec<String>,
    pub cook_time: Option<String>,
    #[serde(with = "paprika_date_format")]
    pub created: chrono::DateTime<chrono::Utc>,
    pub description: Option<String>,
    pub difficulty: Option<String>,
    pub directions: String,
    pub hash: String,
    pub image_url: Option<String>,
    pub in_trash: bool,
    pub ingredients: String,
    pub is_pinned: bool,
    pub name: String,
    pub notes: String,
    pub on_favorites: bool,
    pub on_grocery_list: bool,
    pub photo: Option<String>,
    pub photo_hash: Option<String>,
    pub photo_large: Option<String>,
    pub photo_url: Option<String>,
    pub prep_time: Option<String>,
    pub rating: i32,
    pub scale: Option<String>,
    pub servings: Option<String>,
    pub source: Option<String>,
    pub source_url: Option<String>,
    pub total_time: Option<String>,
    pub uid: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "database", derive(sqlx::FromRow))]
pub struct PaprikaMeal {
    pub uid: String,
    pub recipe_uid: String,
    #[serde(with = "paprika_date_format")]
    pub date: chrono::DateTime<chrono::Utc>,
    #[serde(rename = "type")]
    pub meal_type: i32,
    pub name: String,
    pub order_flag: i32,
    pub type_uid: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "database", derive(sqlx::FromRow))]
pub struct PaprikaGroceryItem {
    pub uid: String,
    pub recipe_uid: Option<String>,
    pub name: String,
    pub order_flag: i32,
    pub purchased: bool,
    pub aisle: String,
    pub ingredient: String,
    pub recipe: Option<String>,
    pub instruction: String,
    pub quantity: String,
    pub separate: bool,
    pub aisle_uid: String,
    pub list_uid: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "database", derive(sqlx::FromRow))]
pub struct PaprikaAisle {
    pub uid: String,
    pub name: String,
    pub order_flag: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "database", derive(sqlx::FromRow))]
pub struct PaprikaMenu {
    pub uid: String,
    pub name: String,
    pub notes: String,
    pub order_flag: i32,
    pub days: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "database", derive(sqlx::FromRow))]
pub struct PaprikaMenuItem {
    pub uid: String,
    pub name: String,
    pub order_flag: i32,
    pub recipe_uid: String,
    pub menu_uid: String,
    pub type_uid: String,
    pub day: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "database", derive(sqlx::FromRow))]
pub struct PaprikaPhoto {
    pub uid: String,
    pub filename: String,
    pub recipe_uid: String,
    pub order_flag: i32,
    pub name: String,
    pub hash: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "database", derive(sqlx::FromRow))]
pub struct PaprikaMealType {
    pub uid: String,
    pub name: String,
    pub order_flag: i32,
    pub color: String,
    pub export_all_day: bool,
    pub export_time: i32,
    pub original_type: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "database", derive(sqlx::FromRow))]
pub struct PaprikaPantryItem {
    pub uid: String,
    pub ingredient: String,
    pub aisle: String,
    #[serde(default, with = "paprika_optional_date_format")]
    pub expiration_date: Option<chrono::DateTime<chrono::Utc>>,
    pub has_expiration: bool,
    pub in_stock: bool,
    #[serde(with = "paprika_date_format")]
    pub purchase_date: chrono::DateTime<chrono::Utc>,
    pub quantity: String,
    pub aisle_uid: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "database", derive(sqlx::FromRow))]
pub struct PaprikaGroceryIngredient {
    pub uid: String,
    pub name: String,
    pub aisle_uid: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "database", derive(sqlx::FromRow))]
pub struct PaprikaGroceryList {
    pub uid: String,
    pub name: String,
    pub order_flag: i32,
    pub is_default: bool,
    pub reminders_list: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "database", derive(sqlx::FromRow))]
pub struct PaprikaBookmark {
    pub uid: String,
    pub title: String,
    pub url: String,
    pub order_flag: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "database", derive(sqlx::FromRow))]
pub struct PaprikaCategory {
    pub uid: String,
    pub order_flag: i32,
    pub name: String,
    pub parent_uid: Option<String>,
}

pub trait PaprikaId {
    fn paprika_id(&self) -> String;
}

impl PaprikaId for PaprikaRecipeHash {
    fn paprika_id(&self) -> String {
        self.uid.to_owned()
    }
}

impl PaprikaId for PaprikaRecipe {
    fn paprika_id(&self) -> String {
        self.uid.to_owned()
    }
}

impl PaprikaId for PaprikaMeal {
    fn paprika_id(&self) -> String {
        self.uid.to_owned()
    }
}

impl PaprikaId for PaprikaGroceryItem {
    fn paprika_id(&self) -> String {
        self.uid.to_owned()
    }
}

impl PaprikaId for PaprikaAisle {
    fn paprika_id(&self) -> String {
        self.uid.to_owned()
    }
}

impl PaprikaId for PaprikaMenu {
    fn paprika_id(&self) -> String {
        self.uid.to_owned()
    }
}

impl PaprikaId for PaprikaMenuItem {
    fn paprika_id(&self) -> String {
        self.uid.to_owned()
    }
}

impl PaprikaId for PaprikaPhoto {
    fn paprika_id(&self) -> String {
        self.uid.to_owned()
    }
}

impl PaprikaId for PaprikaMealType {
    fn paprika_id(&self) -> String {
        self.uid.to_owned()
    }
}

impl PaprikaId for PaprikaPantryItem {
    fn paprika_id(&self) -> String {
        self.uid.to_owned()
    }
}

impl PaprikaId for PaprikaGroceryIngredient {
    fn paprika_id(&self) -> String {
        self.uid.to_owned()
    }
}

impl PaprikaId for PaprikaGroceryList {
    fn paprika_id(&self) -> String {
        self.uid.to_owned()
    }
}

impl PaprikaId for PaprikaBookmark {
    fn paprika_id(&self) -> String {
        self.uid.to_owned()
    }
}

impl PaprikaId for PaprikaCategory {
    fn paprika_id(&self) -> String {
        self.uid.to_owned()
    }
}

fn auth_headers(token: &str) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();

    headers.insert(
        reqwest::header::ACCEPT,
        reqwest::header::HeaderValue::from_static("application/json"),
    );

    let mut auth_value = reqwest::header::HeaderValue::from_str(&format!("Bearer {}", token))
        .expect("token was not valid header value");
    auth_value.set_sensitive(true);
    headers.insert(reqwest::header::AUTHORIZATION, auth_value);

    headers
}

impl PaprikaClient {
    pub async fn login<S: AsRef<str>>(email: S, password: S) -> Result<Self, Error> {
        let client = reqwest::Client::new();

        tracing::trace!("attempting to perform paprika login");
        let req = client
            .post(format!("{}/account/login/", API_ENDPOINT))
            .form(&[("email", email.as_ref()), ("password", password.as_ref())])
            .send()
            .await?
            .error_for_status()?;

        tracing::debug!("got paprika token");
        let PaprikaResult {
            result: PaprikaToken { token },
        } = req.json().await?;

        tracing::trace!("rebuilding http client with authorization headers");
        let client = reqwest::Client::builder()
            .default_headers(auth_headers(&token))
            .build()?;

        Ok(Self { client, token })
    }

    pub async fn token<S: AsRef<str>>(token: S) -> Result<Self, Error> {
        let client = reqwest::Client::builder()
            .default_headers(auth_headers(token.as_ref()))
            .build()?;

        let paprika = Self {
            client,
            token: token.as_ref().to_string(),
        };

        tracing::debug!("checking token validity");
        let _status = paprika.status().await?;

        Ok(paprika)
    }

    pub async fn status(&self) -> Result<PaprikaStatus, Error> {
        let req = self
            .client
            .get(format!("{}/sync/status/", API_ENDPOINT))
            .send()
            .await?
            .error_for_status()?;

        let PaprikaResult { result: status } = req.json().await?;

        Ok(status)
    }

    pub async fn recipes(&self) -> Result<Vec<PaprikaRecipeHash>, Error> {
        let req = self
            .client
            .get(format!("{}/sync/recipes/", API_ENDPOINT))
            .send()
            .await?
            .error_for_status()?;

        let PaprikaResult {
            result: recipe_hashes,
        } = req.json().await?;

        Ok(recipe_hashes)
    }

    pub async fn recipe<S: AsRef<str>>(&self, uid: S) -> Result<PaprikaRecipe, Error> {
        let req = self
            .client
            .get(format!("{}/sync/recipe/{}/", API_ENDPOINT, uid.as_ref()))
            .send()
            .await?
            .error_for_status()?;

        let PaprikaResult { result: recipe } = req.json().await?;

        Ok(recipe)
    }

    pub async fn meals(&self) -> Result<Vec<PaprikaMeal>, Error> {
        let req = self
            .client
            .get(format!("{}/sync/meals/", API_ENDPOINT))
            .send()
            .await?
            .error_for_status()?;

        let PaprikaResult { result: meals } = req.json().await?;

        Ok(meals)
    }

    pub async fn groceries(&self) -> Result<Vec<PaprikaGroceryItem>, Error> {
        let req = self
            .client
            .get(format!("{}/sync/groceries/", API_ENDPOINT))
            .send()
            .await?
            .error_for_status()?;

        let PaprikaResult { result: meals } = req.json().await?;

        Ok(meals)
    }

    pub async fn aisles(&self) -> Result<Vec<PaprikaAisle>, Error> {
        let req = self
            .client
            .get(format!("{}/sync/groceryaisles/", API_ENDPOINT))
            .send()
            .await?
            .error_for_status()?;

        let PaprikaResult { result: aisles } = req.json().await?;

        Ok(aisles)
    }

    pub async fn menus(&self) -> Result<Vec<PaprikaMenu>, Error> {
        let req = self
            .client
            .get(format!("{}/sync/menus/", API_ENDPOINT))
            .send()
            .await?
            .error_for_status()?;

        let PaprikaResult { result: menus } = req.json().await?;

        Ok(menus)
    }

    pub async fn menu_items(&self) -> Result<Vec<PaprikaMenuItem>, Error> {
        let req = self
            .client
            .get(format!("{}/sync/menuitems/", API_ENDPOINT))
            .send()
            .await?
            .error_for_status()?;

        let PaprikaResult { result: menu_items } = req.json().await?;

        Ok(menu_items)
    }

    pub async fn photos(&self) -> Result<Vec<PaprikaPhoto>, Error> {
        let req = self
            .client
            .get(format!("{}/sync/photos/", API_ENDPOINT))
            .send()
            .await?
            .error_for_status()?;

        let PaprikaResult { result: photos } = req.json().await?;

        Ok(photos)
    }

    pub async fn meal_types(&self) -> Result<Vec<PaprikaMealType>, Error> {
        let req = self
            .client
            .get(format!("{}/sync/mealtypes/", API_ENDPOINT))
            .send()
            .await?
            .error_for_status()?;

        let PaprikaResult { result: meal_types } = req.json().await?;

        Ok(meal_types)
    }

    pub async fn pantry_items(&self) -> Result<Vec<PaprikaPantryItem>, Error> {
        let req = self
            .client
            .get(format!("{}/sync/pantry/", API_ENDPOINT))
            .send()
            .await?
            .error_for_status()?;

        let PaprikaResult {
            result: pantry_items,
        } = req.json().await?;

        Ok(pantry_items)
    }

    pub async fn grocery_ingredients(&self) -> Result<Vec<PaprikaGroceryIngredient>, Error> {
        let req = self
            .client
            .get(format!("{}/sync/groceryingredients/", API_ENDPOINT))
            .send()
            .await?
            .error_for_status()?;

        let PaprikaResult {
            result: grocery_ingredients,
        } = req.json().await?;

        Ok(grocery_ingredients)
    }

    pub async fn grocery_lists(&self) -> Result<Vec<PaprikaGroceryList>, Error> {
        let req = self
            .client
            .get(format!("{}/sync/grocerylists/", API_ENDPOINT))
            .send()
            .await?
            .error_for_status()?;

        let PaprikaResult {
            result: grocery_lists,
        } = req.json().await?;

        Ok(grocery_lists)
    }

    pub async fn bookmarks(&self) -> Result<Vec<PaprikaBookmark>, Error> {
        let req = self
            .client
            .get(format!("{}/sync/bookmarks/", API_ENDPOINT))
            .send()
            .await?
            .error_for_status()?;

        let PaprikaResult { result: bookmarks } = req.json().await?;

        Ok(bookmarks)
    }

    pub async fn categories(&self) -> Result<Vec<PaprikaCategory>, Error> {
        let req = self
            .client
            .get(format!("{}/sync/categories/", API_ENDPOINT))
            .send()
            .await?
            .error_for_status()?;

        let PaprikaResult { result: categories } = req.json().await?;

        Ok(categories)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ignore]
    #[tokio::test]
    async fn test_login() {
        let email = std::env::var("PAPRIKA_EMAIL").expect("missing PAPRIKA_EMAIL");
        let password = std::env::var("PAPRIKA_PASSWORD").expect("missing PAPRIKA_PASSWORD");

        let paprika = PaprikaClient::login(email, password)
            .await
            .expect("unable to login");
        println!("token: {}", paprika.token);
    }

    async fn get_paprika() -> PaprikaClient {
        let token = std::env::var("PAPRIKA_TOKEN").expect("tests require PAPRIKA_TOKEN");
        PaprikaClient::token(token)
            .await
            .expect("should be able to use token for authentication")
    }

    #[tokio::test]
    async fn test_token() {
        let _paprika = get_paprika().await;
        println!("authenciated with token");
    }

    #[tokio::test]
    async fn test_status() {
        let paprika = get_paprika().await;
        let status = paprika
            .status()
            .await
            .expect("should be able to get status");
        println!("status: {:#?}", status);
    }

    #[tokio::test]
    async fn test_recipes() {
        let paprika = get_paprika().await;
        let recipes = paprika
            .recipes()
            .await
            .expect("should be able to get recipes");
        println!("recipes: {:#?}", recipes);

        for recipe_hash in recipes {
            let recipe = paprika
                .recipe(&recipe_hash.uid)
                .await
                .expect("recipe should exist");
            println!("recipe: {:#?}", recipe);
        }
    }

    #[tokio::test]
    async fn test_meals() {
        let paprika = get_paprika().await;
        let meals = paprika.meals().await.expect("should be able to get meals");
        println!("meals: {:#?}", meals);
    }

    #[tokio::test]
    async fn test_groceries() {
        let paprika = get_paprika().await;
        let groceries = paprika
            .groceries()
            .await
            .expect("should be able to get groceries");
        println!("groceries: {:#?}", groceries);
    }

    #[tokio::test]
    async fn test_aisles() {
        let paprika = get_paprika().await;
        let aisles = paprika
            .aisles()
            .await
            .expect("should be able to get aisles");
        println!("aisles: {:#?}", aisles);
    }

    #[tokio::test]
    async fn test_menus() {
        let paprika = get_paprika().await;
        let menus = paprika.menus().await.expect("should be able to get menus");
        println!("menus: {:#?}", menus);
    }

    #[tokio::test]
    async fn test_menu_items() {
        let paprika = get_paprika().await;
        let menu_items = paprika
            .menu_items()
            .await
            .expect("should be able to get menu items");
        println!("menu items: {:#?}", menu_items);
    }

    #[tokio::test]
    async fn test_photos() {
        let paprika = get_paprika().await;
        let photos = paprika
            .photos()
            .await
            .expect("should be able to get photos");
        println!("photos: {:#?}", photos);
    }

    #[tokio::test]
    async fn test_meal_types() {
        let paprika = get_paprika().await;
        let meal_types = paprika
            .meal_types()
            .await
            .expect("should be able to get meal types");
        println!("meal types: {:#?}", meal_types);
    }

    #[tokio::test]
    async fn test_pantry() {
        let paprika = get_paprika().await;
        let pantry_items = paprika
            .pantry_items()
            .await
            .expect("should be able to get pantry");
        println!("pantry: {:#?}", pantry_items);
    }

    #[tokio::test]
    async fn test_grocery_ingredients() {
        let paprika = get_paprika().await;
        let grocery_ingredients = paprika
            .grocery_ingredients()
            .await
            .expect("should be able to get grocery ingredients");
        println!("grocery ingredients: {:#?}", grocery_ingredients);
    }

    #[tokio::test]
    async fn test_grocery_lists() {
        let paprika = get_paprika().await;
        let grocery_lists = paprika
            .grocery_lists()
            .await
            .expect("should be able to get grocery lists");
        println!("grocery lists: {:#?}", grocery_lists);
    }

    #[tokio::test]
    async fn test_bookmarks() {
        let paprika = get_paprika().await;
        let bookmarks = paprika
            .bookmarks()
            .await
            .expect("should be able to get bookmarks");
        println!("bookmarks: {:#?}", bookmarks);
    }

    #[tokio::test]
    async fn test_categories() {
        let paprika = get_paprika().await;
        let categories = paprika
            .categories()
            .await
            .expect("should be able to get categories");
        println!("categories: {:#?}", categories);
    }
}
