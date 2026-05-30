mod connections;
mod overview;
mod settings;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Page {
    Overview,
    Connections,
    Settings,
}
