use serde::{Deserialize, Serialize};
use crate::modpack_file::KilnMod;


// Keep separate from KilnFile in case I need to add some other config that doesn't need to be exported/imported
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct ModpackConfig {
    pub name: String,
    pub mods: Vec<KilnMod>,
}