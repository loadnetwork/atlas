use anyhow::Error;
use dotenvy::dotenv;

pub fn get_env_var(key: &str) -> Result<String, Error> {
    dotenv().ok();
    Ok(dotenvy::var(key)?)
}
