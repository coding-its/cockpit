use std::sync::Arc;

use sqlx::PgPool;

use crate::config::Config;
use crate::ninjaone::NinjaOneClient;

pub struct AppState {
    pub pool: PgPool,
    pub ninja: NinjaOneClient,
    pub config: Config,
}

pub type SharedState = Arc<AppState>;
