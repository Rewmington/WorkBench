pub mod app;
pub mod backup;
pub mod commands;
pub mod db;
pub mod error;
pub mod linking;
pub mod model;
pub mod repos;
pub mod secrets;
pub mod seed;
pub mod state;
pub mod suggest;
pub mod sync;
pub mod time;
pub mod ui;
pub mod win32;

pub use app::run;
