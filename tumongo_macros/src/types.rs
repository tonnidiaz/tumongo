use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use strum_macros::{EnumString, VariantNames};

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString, VariantNames, Serialize, Deserialize)]
#[strum(serialize_all = "snake_case")]
pub enum OnDelete{ 
    Null, Cascade
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FkField {
    pub field_name: String,
    /// the collection the field points to
    pub coll: String,
    pub on_delete: Option<String>,
}

/// key = collection the field points to = field.coll
/// 
/// : if dealing with users, we'll access the other collections that contain 'users' as 'fk'
pub type FkFieldMap = HashMap<String, Vec<FkField>>;
