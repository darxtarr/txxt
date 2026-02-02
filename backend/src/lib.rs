
//---------------------------------------
pub mod web_api {
    pub mod routes;
    pub mod controllers;
}

pub use web_api::routes::map_routes;
pub use web_api::controllers::*;
//---------------------------------------

//---------------------------------------
pub mod shared {
    pub mod models;
    pub mod dto;
}

pub use shared::models::*;
pub use shared::dto::*;
//---------------------------------------

//---------------------------------------
pub mod authentication {
    pub mod auth;
}
//---------------------------------------

//---------------------------------------
pub mod data_access {
    pub mod data_context;
}
//---------------------------------------