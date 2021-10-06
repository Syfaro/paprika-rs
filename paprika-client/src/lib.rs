use std::{
    convert::{TryFrom, TryInto},
    io::Write,
};

use serde::{Deserialize, Serialize};

static API_ENDPOINT: &str = "https://www.paprikaapp.com/api/v2";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("encoding error: {0}")]
    Encoding(#[from] serde_json::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("paprika error: {0}")]
    Paprika(#[from] PaprikaError),
}

pub struct PaprikaClient {
    client: reqwest::Client,

    pub token: String,
}

#[derive(Deserialize, Debug, thiserror::Error)]
#[error("{message} (code {code})")]
pub struct PaprikaError {
    pub code: i32,
    pub message: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
enum PaprikaResult<D> {
    Result(D),
    Error(PaprikaError),
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
pub struct PaprikaMeal {
    pub uid: String,
    pub recipe_uid: Option<String>,
    #[serde(with = "paprika_date_format")]
    pub date: chrono::DateTime<chrono::Utc>,
    #[serde(rename = "type")]
    pub meal_type: i32,
    pub name: String,
    pub order_flag: i32,
    pub type_uid: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
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
pub struct PaprikaAisle {
    pub uid: String,
    pub name: String,
    pub order_flag: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PaprikaMenu {
    pub uid: String,
    pub name: String,
    pub notes: String,
    pub order_flag: i32,
    pub days: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
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
pub struct PaprikaPhoto {
    pub uid: String,
    pub filename: String,
    pub recipe_uid: String,
    pub order_flag: i32,
    pub name: String,
    pub hash: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
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
pub struct PaprikaGroceryIngredient {
    pub uid: String,
    pub name: String,
    pub aisle_uid: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PaprikaGroceryList {
    pub uid: String,
    pub name: String,
    pub order_flag: i32,
    pub is_default: bool,
    pub reminders_list: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PaprikaBookmark {
    pub uid: String,
    pub title: String,
    pub url: String,
    pub order_flag: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
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
        let result: PaprikaResult<PaprikaToken> = req.json().await?;
        let token = match result {
            PaprikaResult::Result(token) => token.token,
            PaprikaResult::Error(err) => return Err(err.into()),
        };

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

    async fn json_get<S, D>(&self, endpoint: S) -> Result<D, Error>
    where
        S: AsRef<str>,
        D: serde::de::DeserializeOwned,
    {
        let req = self
            .client
            .get(format!("{}/{}/", API_ENDPOINT, endpoint.as_ref()))
            .send()
            .await?
            .error_for_status()?;

        let result: PaprikaResult<D> = req.json().await?;
        match result {
            PaprikaResult::Result(result) => Ok(result),
            PaprikaResult::Error(err) => Err(err.into()),
        }
    }

    #[allow(dead_code)]
    async fn json_post<S, D>(&self, endpoint: S, data: D) -> Result<(), Error>
    where
        S: AsRef<str>,
        D: serde::Serialize,
    {
        let json = serde_json::to_vec(&data)?;

        let mut compressor =
            flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        compressor.write_all(&json)?;
        let payload = compressor.finish()?;

        let part = reqwest::multipart::Part::bytes(payload).file_name("file");
        let form = reqwest::multipart::Form::default().part("data", part);

        self.client
            .post(format!("{}/{}/", API_ENDPOINT, endpoint.as_ref()))
            .multipart(form)
            .send()
            .await?
            .error_for_status()?;

        Ok(())
    }

    pub async fn status(&self) -> Result<PaprikaStatus, Error> {
        self.json_get("sync/status").await
    }

    pub async fn recipes(&self) -> Result<Vec<PaprikaRecipeHash>, Error> {
        self.json_get("sync/recipes").await
    }

    pub async fn recipe<S: AsRef<str>>(&self, uid: S) -> Result<PaprikaRecipe, Error> {
        self.json_get(format!("sync/recipe/{}", uid.as_ref())).await
    }

    pub async fn meals(&self) -> Result<Vec<PaprikaMeal>, Error> {
        self.json_get("sync/meals").await
    }

    pub async fn groceries(&self) -> Result<Vec<PaprikaGroceryItem>, Error> {
        self.json_get("sync/groceries").await
    }

    pub async fn aisles(&self) -> Result<Vec<PaprikaAisle>, Error> {
        self.json_get("sync/groceryaisles").await
    }

    pub async fn menus(&self) -> Result<Vec<PaprikaMenu>, Error> {
        self.json_get("sync/menus").await
    }

    pub async fn menu_items(&self) -> Result<Vec<PaprikaMenuItem>, Error> {
        self.json_get("sync/menuitems").await
    }

    pub async fn photos(&self) -> Result<Vec<PaprikaPhoto>, Error> {
        self.json_get("sync/photos").await
    }

    pub async fn meal_types(&self) -> Result<Vec<PaprikaMealType>, Error> {
        self.json_get("sync/mealtypes").await
    }

    pub async fn pantry_items(&self) -> Result<Vec<PaprikaPantryItem>, Error> {
        self.json_get("sync/pantry").await
    }

    pub async fn grocery_ingredients(&self) -> Result<Vec<PaprikaGroceryIngredient>, Error> {
        self.json_get("sync/groceryingredients").await
    }

    pub async fn grocery_lists(&self) -> Result<Vec<PaprikaGroceryList>, Error> {
        self.json_get("sync/grocerylists").await
    }

    pub async fn bookmarks(&self) -> Result<Vec<PaprikaBookmark>, Error> {
        self.json_get("sync/bookmarks").await
    }

    pub async fn categories(&self) -> Result<Vec<PaprikaCategory>, Error> {
        self.json_get("sync/categories").await
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
        let _ = tracing_subscriber::fmt::try_init();
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
