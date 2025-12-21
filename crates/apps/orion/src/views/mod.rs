//! GPUI view components for Orion mail app

mod thread;
mod thread_list;

pub use thread::{generate_thread_html, ThreadView};
pub use thread_list::ThreadListView;
