pub mod web_api {
    pub mod routes;
    pub mod controllers;
}

pub mod shared {
    pub mod models;
}

pub use web_api::controllers::*;
pub use web_api::routes::map_routes;
pub use shared::models::*;