use darling::{FromDeriveInput, FromField};
use proc_macro::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use strum::VariantNames;
use syn::DeriveInput;

use crate::{
    types::{FkField, OnDelete}, FK_FIELDS, REF_FIELDS
};
// use tumongo::FkField;

#[derive(Debug, FromDeriveInput)]
#[darling(attributes(tumongo), supports(struct_named))]
struct StructOpts {
    coll_name: String,
    ident: syn::Ident,
    data: darling::ast::Data<(), FieldOpts>,
}

#[derive(Debug, FromField)]
#[darling(attributes(tumongo))]
struct FieldOpts {
    ident: Option<syn::Ident>,
    ty: syn::Type,
    #[darling(default)]
    unique: bool,
    /// unique if same same field = field.value
    /// e.g name is unique if same user_id
    #[darling(default)]
    unique_if_same: Option<String>,
    #[darling(default)]
    fk: bool,

    /// ref field
    #[darling(default)]
    reff: bool,
    #[darling(default)]
    coll: Option<String>,
    #[darling(default)]
    on_delete: Option<String>,
}

pub fn main(input: DeriveInput) -> TokenStream {
    let opts = match StructOpts::from_derive_input(&input) {
        Ok(o) => o,
        Err(e) => {
            return e.write_errors().into();
        }
    };

    let struct_name = &opts.ident;

    let coll_name = &opts.coll_name; // the struct's collection name
    // let mut field_names = vec![];
    let mut fk_fields = vec![];
    let mut ref_fields = vec![];
    // [(name, type)]
    let mut field_types = vec![];
    let mut field_names = vec![];
    let mut unique_fields = vec![];
    let mut unique_if_same_fields = vec![];

    let on_delete_vals = OnDelete::VARIANTS;

    opts.data
        .as_ref()
        .take_struct()
        .unwrap()
        .fields
        .iter()
        .for_each(|f| {
            let name = f.ident.as_ref().unwrap().to_string();
            let _ty = &f.ty;
            let field_name_ident = format_ident!("{}", name);
            field_names.push(field_name_ident.clone());
            let ty_tkn = quote! {stringify!(#_ty)};
            field_types.push(ty_tkn.clone());
            if name == "created_at" || name == "updated_at" {
                if !ty_tkn.to_string().contains("DateTime") {
                    panic!("created_at and updated_at fields should be of type Tumongo::DateTime");
                }
            }
            if f.unique{
                unique_fields.push(field_name_ident.clone());
            }
            if let Some(same_field) = f.unique_if_same.as_ref(){
                let same_field_ident = format_ident!("{}", same_field);
                unique_if_same_fields.push(quote!{
                    let field_val = &self.#field_name_ident;
                    let same_field_val = &self.#same_field_ident;
                    if coll.find_one(doc!{#name: field_val, "_id": {"$ne": self.id}, #same_field: same_field_val}).await?.is_some(){
                        return Err(format!("UNIQUE FIELD ERROR: another doc with [{} = {field_val:?}] and [{} = {same_field_val:?}]  already exists in {} collection.", #name, #same_field, Self::coll_name()).into()); 
                    }

                });
            }
            if f.fk {
                if f.coll.is_none() {
                    panic!("Specify collection name (coll)");
                }
                if let Some(ref val) = f.on_delete {
                    if !on_delete_vals.contains(&val.as_str()) {
                        panic!(
                            "Invalid value({val}) for {name}.\nValid values are: {on_delete_vals:?}"
                        );
                    }
                }
                let mut fk_field = FkField {
                    field_name: name.clone(),
                    coll: f.coll.as_ref().unwrap().to_string(),
                    on_delete: f.on_delete.clone(),
                };

                let mut reg = FK_FIELDS.lock().expect("Failed to lock regg.");
                let parent_col = fk_field.coll.clone();
                fk_field.coll = coll_name.clone();
                if let Some(fk_colls) = reg.get_mut(&parent_col) {
                    fk_colls.push(fk_field.clone());
                } else {
                    reg.insert(parent_col.clone(), vec![fk_field.clone()]);
                }
                let fk_field = serde_json::to_string(&fk_field).unwrap();
                fk_fields.push(fk_field);
            }
            if f.reff {
                if f.coll.is_none() {
                    panic!("Specify collection name (coll)");
                }
                if let Some(ref val) = f.on_delete {
                    if !on_delete_vals.contains(&val.as_str()) {
                        panic!(
                            "Invalid value({val}) for {name}.\nValid values are: {on_delete_vals:?}"
                        );
                    }
                }
                let ref_field = FkField {
                    field_name: name.clone(),
                    coll: f.coll.as_ref().unwrap().to_string(),
                    on_delete: f.on_delete.clone(),
                };
                let mut reg = REF_FIELDS.lock().expect("Failed to lock refs reg.");
                let parent_col = coll_name.clone();
                if let Some(ref_colls) = reg.get_mut(&parent_col) {
                    ref_colls.push(ref_field.clone());
                } else {
                    reg.insert(parent_col.clone(), vec![ref_field.clone()]);
                }
                let ref_field = serde_json::to_string(&ref_field).unwrap();
                ref_fields.push(ref_field);
            }
        });
    let required_fields = ["id", "created_at", "updated_at"];
    let required_fields_exist = required_fields
        .iter()
        .all(|x| field_names.iter().any(|y| &y.to_string() == x));
    if !required_fields_exist {
        panic!("struct should contain {required_fields:?}");
    }
    let unique_fields_str: Vec<String> = unique_fields.iter().map(|x| x.to_string()).collect();
    let mod_name = format_ident!("__{}__", struct_name.to_string().to_lowercase());

    let expanded = quote_spanned! { struct_name.span()=>

        mod #mod_name {
            use tumongo::{ Tumongo,
                db::{self, bson::{self, doc, Document}, Database},
                 futures_util::TryStreamExt,
                serde_json::{self,Value}
            };
            type Res<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

            impl super::#struct_name {
                pub fn coll_name() -> String {
                    #coll_name.to_string()
                }
                pub fn collection(db: &db::Database) -> db::Collection<Self> {
                    let coll_name = Self::coll_name();
                    db.collection::<Self>(&coll_name)
                }

                pub fn get_fields() -> Vec<String>{
                    vec![#(stringify!(#field_names).to_string()),*]
                }

                pub fn has_field(f: &String) -> bool{
                    Self::get_fields().contains(f)
                }
                pub fn set_field(&mut self, f: &str, v: Value) -> bool{
                    let mut ok = true;
                    // self.to_value().
                        match f{
                            #(
                                stringify!(#field_names) => {
                                    if let Ok(boxed) = serde_json::from_value(v){
                                        self.#field_names = boxed;
                                    }else{
                                        ok = false;
                                    }
                                }
                            )*

                        _=> ok = false
                        }
                    ok
                }

                
                pub fn test(){
                    println!("\n[TEST] {:#?}", Tumongo::fk_fields());
                }

                pub async fn find(
                    _db: &Database,
                    filter: Document, skip: Option<u64>, limit: Option<i64>
                ) -> Res<Vec<Self>> {
                    let coll = _db
                    .collection::<Document>(&Self::coll_name());

                    let mut query = coll
                    .find(filter.clone());
                    
                    if let Some(skip) = skip{
                        query = query.skip(skip);
                    }
                    if let Some(limit) = limit{
                        query = query.limit(limit);
                    }
                    let mut res = query
                        .await?;

                    let mut res_doks = vec![];

                    while let Some(dok) = res.try_next().await? {
                        let _id = dok.get_object_id("_id").expect("no _id");

                        match bson::from_document::<Self>(dok.clone()){
                        Ok(mut dok) =>{
                            dok.id.replace(_id);
                            res_doks.push(dok);
                        },
                        Err(err)=>{
                            eprintln!("[find] [{}] Error serializing dok {dok:?}", Self::coll_name());
                            eprintln!("{err:?}");
                        }}

                    };

                    Ok(res_doks)
                }
                    pub async fn find_one(
                        _db: &Database,
                        filter: Document,
                    ) -> Res<Self> {
                        let res = _db
                            .collection::<Document>(&Self::coll_name())
                            .find_one(filter.clone())
                            .await?;
                        if let Some(dok) = res {
                            let _id = dok.get_object_id("_id").unwrap();
                            let mut dok: Self = bson::from_document(dok.clone()).expect(&format!("Failed to convert {dok:?} to {:?}", stringify!(#struct_name)));
                            dok.id.replace(_id);
                            Ok(dok)
                        } else {
                            return Err(format!("No matching item in {} for {filter:?}", Self::coll_name()).into());
                        }
                    }

                    /// also updates the instance's id field
                    pub async fn save(&mut self, db: &db::Database) -> Res<()> {
                        
                        let coll = Self::collection(db);
                        #(
                            let _val = &self.#unique_fields;
                            if coll.find_one(doc!{#unique_fields_str: _val, "_id": {"$ne": self.id}}).await?.is_some(){
                                return Err(format!("UNIQUE FIELD ERROR: another doc with {} = {_val:?} already exists in {} collection.", #unique_fields_str, Self::coll_name()).into()); 
                            }
                        )*

                        // handle unique_if_same fields
                        #(#unique_if_same_fields)*

                        self.updated_at = tumongo::DateTime::now();
                        let tx = if let Some(ref _id) = self.id {
                            coll.update_one(
                                doc! {  "_id": _id },
                                doc! {
                                    "$set": self.to_doc()
                                },
                            )
                            .await?;
                            _id.clone()
                        } else {
                            self.created_at = tumongo::DateTime::now();
                            println!("[new_acc] created_at: {:?}", self.created_at);
                            coll.insert_one(&*self)
                                .await?
                                .inserted_id
                                .as_object_id()
                                .unwrap()
                        };

                        println!("[new_acc] created_at: {:?}\n", self.created_at);


                        self.id.replace(tx);
                        Ok(())
                    }

                    pub async fn insert_many(
                        db: &db::Database,
                        list: &Vec<Self>,
                    ) -> Result<db::results::InsertManyResult, db::error::Error> {
                        Self::collection(db).insert_many(list).await
                    }
                    pub fn to_value(&self) -> serde_json::Value {
                        serde_json::to_value(self).expect("Unvaluable")
                    }
                    pub fn to_doc(&self) -> Document {
                        let mut dok = Document::try_from(self.to_value().as_object().expect("Can't convert value to obj").clone()).expect("Can't create document");
                        if let Some(_id) = self.id {
                            dok.insert("_id", _id);
                        }

                        dok
                    }

                    pub async fn delete(&self, db: &db::Database) -> db::error::Result<()> {
                        if self.id.is_none(){
                            return Ok(());
                        }
                        let mut sess = db.client().start_session().await?;
                        let fk_fields = Tumongo::fk_fields();
                        let coll_name = Self::coll_name();
                        Tumongo::delete(&db, &coll_name, &self.id.unwrap(), &fk_fields, &mut sess, false).await
                    }

          /*           /// coll_name: target collection name
                pub async fn populate(
                    &self,
                    _db: &Database,
                    coll_names: Option<&[&str]>,
                ) -> Document {
                    let mut data: HashMap<String, Vec<Document>> = HashMap::new();
                    let _id = self.id.as_ref().expect("No self.id");

                    if let Some(fks) = Tumongo::fk_fields().get(&Self::coll_name()) {
                        for fk in fks.iter() {
                            let fk_coll = fk.coll.as_str();
                            if coll_names.is_some() && !coll_names.expect("coll_names none").contains(&fk_coll) {
                                continue;
                            }
                            let field = &fk.field_name;
                            let dok_cursor = _db
                                .collection::<Document>(&fk.coll)
                                .find(doc! {
                                    field: _id
                                })
                                .await;
                            if let Ok(dok_cursor) = dok_cursor {
                                let doks: Vec<_> = dok_cursor.try_collect().await.unwrap_or_else(|_| vec![]);
                                data.insert(fk_coll.to_owned(), doks);
                            }
                        }
                    }

                    let mut doc_dok = self.to_doc();
                    for (k, v) in data{
                        doc_dok.insert(k, v);
                    }
                    doc_dok
                }
             */
            
            pub async fn populate(&self, _db: &Database, coll_names: Option<&[&str]>) -> Document {
                Tumongo::populate(
                    &_db,
                    self.to_doc(),
                    &Self::coll_name(),
                    coll_names,
                    &Tumongo::fk_fields(),
                    &Tumongo::ref_fields(),
                )
                .await
            }
            }
        }

    };
    TokenStream::from(expanded)
}
