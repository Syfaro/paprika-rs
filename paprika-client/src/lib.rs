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
        let s = format!("{}", date.format(FORMAT));
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

#[derive(Clone, Debug, Deserialize)]
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
    pub recipe_uid: String,
    #[serde(with = "paprika_date_format")]
    pub date: chrono::DateTime<chrono::Utc>,
    #[serde(rename = "type")]
    pub meal_type: i32,
    pub name: String,
    pub order_flag: i32,
    pub type_uid: String,
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

pub trait PaprikaCompare {
    fn paprika_compare(&self, rhs: &Self) -> bool;
}

impl PaprikaCompare for PaprikaRecipeHash {
    fn paprika_compare(&self, rhs: &Self) -> bool {
        self.hash == rhs.hash
    }
}

impl PaprikaCompare for PaprikaRecipe {
    fn paprika_compare(&self, rhs: &Self) -> bool {
        self.hash == rhs.hash
    }
}

impl PaprikaCompare for PaprikaMeal {
    fn paprika_compare(&self, rhs: &Self) -> bool {
        self == rhs
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

    pub async fn recipe_list(&self) -> Result<Vec<PaprikaRecipeHash>, Error> {
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

    pub async fn meal_list(&self) -> Result<Vec<PaprikaMeal>, Error> {
        let req = self
            .client
            .get(format!("{}/sync/meals/", API_ENDPOINT))
            .send()
            .await?
            .error_for_status()?;

        let PaprikaResult { result: meals } = req.json().await?;

        Ok(meals)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get_token() -> String {
        std::env::var("PAPRIKA_TOKEN").expect("tests require PAPRIKA_TOKEN")
    }

    #[tokio::test]
    async fn test_token() {
        let _paprika = PaprikaClient::token(&get_token())
            .await
            .expect("should be able to use token for authentication");
        println!("authenciated with token");
    }

    #[tokio::test]
    async fn test_status() {
        let paprika = PaprikaClient::token(&get_token())
            .await
            .expect("should be able to use token for authentication");
        let status = paprika
            .status()
            .await
            .expect("should be able to get status");
        println!("status: {:?}", status);
    }

    #[tokio::test]
    async fn test_recipes() {
        let paprika = PaprikaClient::token(&get_token())
            .await
            .expect("should be able to use token for authentication");
        let recipe_list = paprika
            .recipe_list()
            .await
            .expect("should be able to get recipe list");
        println!("recipe list: {:?}", recipe_list);

        for recipe_hash in recipe_list {
            let recipe = paprika
                .recipe(&recipe_hash.uid)
                .await
                .expect("recipe should exist");
            println!("recipe: {:?}", recipe);
        }
    }

    #[tokio::test]
    async fn test_meal_list() {
        let paprika = PaprikaClient::token(&get_token())
            .await
            .expect("should be able to use token for authentication");
        let meal_list = paprika
            .meal_list()
            .await
            .expect("should be able to get meal list");
        println!("meal list: {:#?}", meal_list);
    }
}
