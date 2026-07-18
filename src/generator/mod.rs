//! Code generation now lives in the headless SDK; the GUI re-exports it and
//! drives it from the editable `Field` tree (see [`crate::gui::GeneratorWindow`]).

pub use nemclass_sdk::generator::Generator;

/// The GUI's generator selector is the SDK's [`nemclass_sdk::Lang`].
pub use nemclass_sdk::Lang as AvailableGenerator;
