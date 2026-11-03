//! Grey City simulation core. No graphics in here: the renderer (and the
//! headless runner) drive the `World` through its public API.

pub mod ai;
pub mod combat;
pub mod events;
pub mod fire;
pub mod map;
pub mod path;
pub mod person;
pub mod player;
pub mod social;
pub mod world;

pub use map::Pos;
pub use world::World;
