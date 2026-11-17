//! Which part of a program has the keyboard focus (an IDE's terminal, editor, AI chat, …), so a
//! profile's extra set of buttons for that panel can be chosen. Ports of the pure half of
//! `Services/Context/*`: the catalog, the scoring, the stabiliser. Reading the accessibility tree
//! is the OS layer's job (`oasis-win`).

pub mod catalog;
pub mod classify;
pub mod state;

pub use classify::{
    classify, merge_workbench, should_probe_workbench, workbench_panel, PanelResult, UiNode, UiSnapshot,
    WorkbenchNode,
};
pub use state::PanelState;
