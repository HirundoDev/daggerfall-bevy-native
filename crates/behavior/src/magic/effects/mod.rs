//! Effect template tables of M23 (contract §4).
//!
//! One module per DFU `Assets/Scripts/Game/MagicAndEffects/Effects/<Family>/`
//! folder, one row per registered template, every row citing the C# file and
//! line it comes from. The engine lives in [`crate::magic`]: these modules are
//! **data only** (`&'static [EffectProperties]`), so a template is added by
//! adding a row — no code path, no match arm.
//!
//! Census (`contract §1`): 153 source files = 143 concrete templates + 10
//! abstract bases that DFU never registers; expanding the variant families
//! (`ElementalResistance`, `MageLight`, `Pacify`, `Poison`) yields **165
//! registered keys**.

pub mod alteration;
pub mod destruction;
pub mod diseases;
pub mod enchanting;
pub mod illusion;
pub mod mysticism;
pub mod poisons;
pub mod restoration;
pub mod special;
pub mod thaumaturgy;

pub use alteration::ALTERATION;
pub use destruction::DESTRUCTION;
pub use diseases::DISEASES;
pub use enchanting::ENCHANTING;
pub use illusion::ILLUSION;
pub use mysticism::MYSTICISM;
pub use poisons::POISONS;
pub use restoration::RESTORATION;
pub use special::SPECIAL;
pub use thaumaturgy::THAUMATURGY;
