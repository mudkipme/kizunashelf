mod analytics;
mod assets;
mod entities;
mod episodes;
mod error;
pub(crate) mod external;
mod frontmatter_draft;
mod handlers;
mod import;
mod lists;
mod log;
mod mutations;
mod path_suggestions;
mod router;
mod smart_lists;
mod state;
mod tags;
pub mod tunnel;

pub use error::{ApiError, ApiResult};
// Static registry facts for the `kizunashelf-docs` generator (bin/docs.rs).
pub use external::provider_credential_keys;
pub use external::{provider_catalog_items, provider_episode_support};
pub use router::{openapi, router_native, router_with_vault};
pub use state::ApiOptions;
