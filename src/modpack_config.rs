use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct ModpackConfig {
    pub name: String,
    pub mods: HashMap<String, String>,
}