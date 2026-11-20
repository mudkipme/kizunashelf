//! HTTP-level integration tests: each module drives the real router over a
//! temp vault for one area of the API. Shared fixtures live in `support`.

mod support;

mod assets;
mod calendar_log;
mod edit_review;
mod entities;
mod episodes;
mod external;
mod home;
mod import;
mod lists;
mod presets;
mod ratings_activity;
mod settings;
mod system;
mod tags;
