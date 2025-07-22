use crossbeam_channel::{Sender};

#[derive(Debug)]
pub enum PluginEffect {
    Echo(String),
}
