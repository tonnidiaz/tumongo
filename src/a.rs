use std::collections::HashMap;
use crate::{FK_FIELDS, FkFieldMap, REF_FIELDS, Tumongo, UNIQUE_FIELDS};

impl Tumongo {
    pub fn unique_fields() -> &'static HashMap<String, Vec<String>> {
        UNIQUE_FIELDS.get().expect("NO UNIQUE")
    }
    pub fn fk_fields() -> &'static FkFieldMap {
        FK_FIELDS.get().expect("no FK_FIELDS..")
    }
    pub fn ref_fields() -> &'static FkFieldMap {
        REF_FIELDS.get().expect("no REF_FIELDS")
    }
}
