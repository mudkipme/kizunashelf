mod analytics;
mod assets;
mod entities;
mod entity_edit_review;
mod episodes;
mod error;
pub(crate) mod external;
mod frontmatter_draft;
mod handlers;
mod import;
mod list_files;
mod lists;
mod log;
mod mutations;
mod path_suggestions;
mod ratings;
mod router;
mod smart_lists;
mod state;
mod tags;
mod tasks;
pub mod tunnel;
#[cfg(not(target_os = "ios"))]
mod web_auth;

pub use error::{ApiError, ApiResult};
// Static registry facts for the `kizunashelf-docs` generator (bin/docs.rs).
pub use external::provider_credential_keys;
pub use external::{provider_catalog_items, provider_episode_support};
pub use router::{openapi, router_native, router_with_vault};
pub use state::ApiOptions;
#[cfg(not(target_os = "ios"))]
pub use web_auth::{protect_web_router, WebAuth};
