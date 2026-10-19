//! Geometry kernel for HouseCraft.
//!
//! All coordinates are `f64` millimetres in `glam` vectors. Functions here
//! read a [`Document`](housecraft_model::Document) and derive geometry from it.
//! They never mutate it, so the API crate decides what to store.

pub mod boolean;
pub mod cabinets;
pub mod electrical;
pub mod framing;
pub mod math;
pub mod mesh;
pub mod pattern;
pub mod piers;
pub mod roof;
pub mod rooms;
pub mod site;
pub mod skeleton;
pub mod solid;
pub mod stairs;
pub mod sun;
pub mod sweep;
pub mod terrain;
pub mod transform;
pub mod trim;
pub mod triangulate;
pub mod walls;

pub use glam::{DMat2, DVec2, DVec3};
pub use math::{pt, v2};
pub use transform::Transform2;
