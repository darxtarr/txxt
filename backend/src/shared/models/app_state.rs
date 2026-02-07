use std::sync::Arc;
// use tokio::sync::broadcast::Sender;
use crate::data_access::data_context::DataContext;

pub struct AppState {
    pub data_context: DataContext,
    //pub ws_broadcast: Sender<String>,
}

pub type SharedState = Arc<AppState>;