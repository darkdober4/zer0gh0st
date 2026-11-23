pub mod config;
pub mod error;
pub mod imap_client;
pub mod parser;
pub mod service;
pub mod smtp_client;
pub mod types;

use crate::email::config::EmailConfig;
use crate::email::service::EmailService;

pub async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = EmailConfig::from_env()?;
    let _service = EmailService::new(config)?;
    Ok(())
}
