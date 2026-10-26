// src/services/state.rs --
// 2026-01-04 09:40

////////

use crate::config::Settings;
use crate::service::mongodb::MessageStoreBackend;

////////

pub type AppState = app_config::app_state::AppState<Settings, MessageStoreBackend>;

//////// END
