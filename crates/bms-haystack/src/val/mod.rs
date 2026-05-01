//! Haystack value types — the runtime representation of every value kind
//! defined by Project Haystack 5 (`Marker`, `Number`, `Ref`, `Grid`, etc.).
//! Codecs in [`crate::codec`] convert between [`Value`] and wire formats
//! (Hayson JSON, Zinc text).

pub mod coord;
pub mod datetime;
pub mod dict;
pub mod grid;
pub mod number;
pub mod ref_;
pub mod value;

pub use coord::Coord;
pub use datetime::{HDate, HDateTime, HTime};
pub use dict::Dict;
pub use grid::{Col, Grid};
pub use number::Number;
pub use ref_::Ref;
pub use value::{Value, XStr};
