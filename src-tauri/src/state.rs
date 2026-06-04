use std::path::PathBuf;
use tokio::sync::Mutex;

use crate::db::Database;
use crate::services::backend::BackendServices;

pub struct AppState {
    pub db: Mutex<Database>,
    pub services: BackendServices,
    pub data_dir: PathBuf,
}
