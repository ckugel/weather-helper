//! weather-helper binary
//!
//! Thin CLI wrapper around the library. Parses the root argument and invokes
//! `weather_helper::run`.

use anyhow::Result;
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    let root = env::args().nth(1).unwrap_or_else(|| ".".to_string());
    weather_helper::run(&root).await
}
