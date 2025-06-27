use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct KilnFile {
    pub name: String,
    pub mods: Vec<KilnMod>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Ord, PartialOrd, Eq)]
#[serde(untagged)]
pub enum KilnMod {
    ModDbMod { id: String, version: String },
    OtherMod { name: String, source: String },
}
