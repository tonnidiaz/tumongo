mod tumongo_model;
mod types;

use once_cell::sync::Lazy;
use proc_macro::TokenStream;
use quote::quote;
use std::{collections::HashMap, sync::Mutex};
use syn::{DeriveInput, parse_macro_input};
use types::FkFieldMap;

static FK_FIELDS: Lazy<Mutex<FkFieldMap>> = Lazy::new(|| Mutex::new(HashMap::new()));
static REF_FIELDS: Lazy<Mutex<FkFieldMap>> = Lazy::new(|| Mutex::new(HashMap::new()));
/// key = coll_name
static UNIQUE_FIELDS: Lazy<Mutex<HashMap<String, Vec<String>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
#[proc_macro_derive(TumongoModel, attributes(tumongo))]
pub fn tumongo_model_macro(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    tumongo_model::main(input)
}

#[proc_macro]
pub fn register(_inp: TokenStream) -> TokenStream {
    let fk_fields = FK_FIELDS.lock().unwrap().clone();
    let fk_fields_str = serde_json::to_string(&fk_fields).unwrap();

    let ref_fields = REF_FIELDS.lock().unwrap().clone();
    let ref_fields_str = serde_json::to_string(&ref_fields).unwrap();

    let unique_fields = UNIQUE_FIELDS.lock().unwrap().clone();
    let unique_fields_str = serde_json::to_string(&unique_fields).unwrap();
    quote! {
        FK_FIELDS.set(serde_json::from_str(#fk_fields_str).unwrap()).unwrap();
        REF_FIELDS.set(serde_json::from_str(#ref_fields_str).unwrap()).unwrap();
        UNIQUE_FIELDS.set(serde_json::from_str(#unique_fields_str).unwrap()).unwrap();
    }
    .into()
}
