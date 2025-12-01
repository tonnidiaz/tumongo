pub mod a;
mod traits;
pub use futures_util;
use futures_util::{TryStreamExt, lock::Mutex};
pub use mongodb as db;
use mongodb::{
    Client, ClientSession, Database,
    bson::{self, Document, doc, oid::ObjectId},
};
pub use once_cell;
use once_cell::sync::OnceCell;
use serde::de::Visitor;
pub use serde::{Deserialize, Serialize};
pub use serde_json;
use std::{collections::HashMap, env, ops::Deref, str::FromStr, sync::Arc};
use strum_macros::{EnumString, VariantNames};
pub use tumongo_macros::*;
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString, VariantNames, Serialize, Deserialize)]
#[strum(serialize_all = "snake_case")]
pub enum OnDelete {
    Null,
    Cascade,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FkField {
    pub field_name: String,
    pub coll: String,
    pub on_delete: Option<String>,
}

pub type FkFieldMap = HashMap<String, Vec<FkField>>;

pub static FK_FIELDS: OnceCell<FkFieldMap> = OnceCell::new();
pub static REF_FIELDS: OnceCell<FkFieldMap> = OnceCell::new();
pub static UNIQUE_FIELDS: OnceCell<HashMap<String, Vec<String>>> = OnceCell::new();
pub static DB: OnceCell<Database> = OnceCell::new();
type SyncDoc = Arc<Mutex<Document>>;
pub type Res<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub struct Tumongo;

impl Tumongo {
    pub async fn delete(
        db: &Database,
        coll_name: &str,
        id: &ObjectId,
        fk_fields: &FkFieldMap,
        sess: &mut ClientSession,
        is_child: bool,
    ) -> db::error::Result<()> {
        if let Some(child_colls) = fk_fields.get(coll_name) {
            for item in child_colls {
                let field_name = &item.field_name;

                let on_delete = item
                    .on_delete
                    .as_ref()
                    .and_then(|s| OnDelete::from_str(s).ok());
                let mut delete = false;

                if let Some(on_delete) = on_delete {
                    match on_delete {
                        OnDelete::Null => {
                            println!("\nClearing {} from {}...", field_name, item.coll);
                            // item: tri_order, field_name: order_a
                            let collection = db.collection::<Document>(&item.coll);
                            let tx = collection
                                .update_many(
                                    doc! { field_name: Some(id) },
                                    doc! {
                                        "$set": doc! { field_name: None::<ObjectId> }
                                    },
                                )
                                .session(&mut *sess)
                                .await?;
                            println!(
                                "\n{} {} cleared from {} collection",
                                tx.modified_count, field_name, item.coll
                            );
                        }
                        OnDelete::Cascade => delete = true,
                    };
                }

                if !delete {
                    continue;
                }
                println!("\n[{}] Deleting child {:?}", coll_name, item);
                let collection = db.collection::<Document>(&item.coll);
                let mut cursor = collection.find(doc! {field_name: id }).await?;
                while let Ok(Some(dok)) = cursor.try_next().await {
                    Box::pin(async {
                        Self::delete(
                            &db,
                            &item.coll,
                            &dok.get_object_id("_id").unwrap(),
                            &fk_fields,
                            sess,
                            true,
                        )
                        .await
                    })
                    .await?;
                }
            }
        }
        let collection = db.collection::<Document>(coll_name);
        if !is_child {
            println!("\nNow deleting {coll_name}...");
        }
        collection
            .delete_one(doc! {"_id": id})
            .session(&mut *sess)
            .await?;
        Ok(())
    }
    async fn populate_ref_fields(
        _db: &Database,
        dok: SyncDoc,
        coll_name: &str,
        coll_names: Option<&[&str]>,
        ref_fields: &FkFieldMap,
    ) -> SyncDoc {
        // log!("[REF_F] {coll_name}");
        if let Some(refs) = ref_fields.get(coll_name) {
            for reff in refs.iter() {
                let ref_coll = reff.coll.as_str();
                // log!("[REF] Populating [{ref_coll}] for [{coll_name}]...");

                let field = &reff.field_name;

                let ref_id = dok.lock().await.get_object_id(field);
                if ref_id.is_err()
                    || coll_names.is_some() && !coll_names.unwrap().contains(&ref_coll)
                {
                    continue;
                }
                let ref_id = ref_id.unwrap();

                let _dok = _db
                    .collection::<Document>(&reff.coll)
                    .find_one(doc! {
                        "_id": ref_id
                    })
                    .await;
                if let Ok(Some(mut _dok)) = _dok {
                    // log!("[REF] [{ref_coll}] for [{coll_name}] id: {ref_id:?}");
                    let _dok = Arc::new(Mutex::new(_dok));
                    let dok_populated = Box::pin(async {
                        Tumongo::populate_ref_fields(_db, _dok.clone(), ref_coll, None, ref_fields)
                            .await
                    })
                    .await;
                    let dok_populated = dok_populated.lock().await.clone();
                    _dok.lock().await.extend(dok_populated);
                    let _dok = _dok.lock().await.clone();
                    dok.lock().await.insert(field.to_owned(), _dok);
                }
            }
        }
        dok
    }

    async fn populate_fk_fields(
        _db: &Database,
        dok: SyncDoc,
        coll_name: &str,
        coll_names: Option<&[&str]>,
        fk_fields: &FkFieldMap,
        ref_fields: &FkFieldMap,
        skip: &mut Vec<String>,
    ) -> SyncDoc {
        // let mut ret = Map::new();
        // log!("[{coll_name}] {:?}", dok);
        if let Some(fks) = fk_fields.get(coll_name) {
            // log!("GETTING DID...");
            let dok_id = dok.lock().await.get_object_id("_id").unwrap();
            // log!("DID: {:?}", dok_id);
            for fk in fks.iter() {
                let fk_coll = fk.coll.as_str();
                // log!("AT [{fk_coll}], SKIP: {skip:?}");
                if skip.contains(&fk.coll) {
                    continue;
                }

                // log!("[FK] Populating [{fk_coll}] for [{coll_name}]...");
                if coll_names.is_some() && !coll_names.unwrap().contains(&fk_coll) {
                    continue;
                }
                let field = &fk.field_name;
                let dok_cursor = _db
                    .collection::<Document>(&fk.coll)
                    .find(doc! {
                        field: dok_id
                    })
                    .await;
                if let Ok(mut dok_cursor) = dok_cursor {
                    let mut doks: Vec<_> = vec![];
                    while let Ok(Some(mut _dok)) = dok_cursor.try_next().await {
                        let _id = _dok.get_object_id("_id").unwrap();
                        // log!("[FK] [{fk_coll}] for [{coll_name}] id: {_id:?}");
                        let _dok = SyncDoc::new(Mutex::new(_dok));
                        let _dok = Self::populate_ref_fields(
                            _db,
                            _dok.clone(),
                            fk_coll,
                            coll_names,
                            ref_fields,
                        )
                        .await;

                        let dok_populated = Box::pin({
                            // let dok = dok.clone();
                            async {
                                Tumongo::populate_fk_fields(
                                    _db,
                                    _dok.clone(),
                                    fk_coll,
                                    None,
                                    fk_fields,
                                    ref_fields,
                                    skip,
                                )
                                .await
                            }
                        })
                        .await;
                        // log!("[{coll_name}] FROM RET");
                        // let mut _dok = serde_json::to_value(_dok).unwrap();
                        // let _dok = _dok.as_object_mut().unwrap();
                        let dok_populated = dok_populated.lock().await.clone();
                        // log!("[{coll_name}] DOK POP");
                        _dok.lock().await.extend(dok_populated);
                        // log!("[{coll_name}] EXTENDED");
                        let _dok = _dok.lock().await.clone();
                        doks.push(_dok);
                        // log!("[{coll_name}] PUSHED");
                    }
                    if !doks.is_empty() {
                        skip.push(fk_coll.to_string());
                    }
                    dok.lock().await.insert(fk_coll.to_owned(), doks);
                }
            }
        }

        // log!("[{coll_name}] returning...");
        dok.clone()
        // ret
    }

    /// populates both fk and ref fields
    pub async fn populate(
        _db: &Database,
        dok: Document,
        coll_name: &str,
        coll_names: Option<&[&str]>,
        fk_fields: &FkFieldMap,
        ref_fields: &FkFieldMap,
    ) -> Document {
        // populate foreign key referencing collections
        // log!("\nREF_FIELDS: {ref_fields:#?}");
        let dok = Arc::new(Mutex::new(dok));

        // populate foreign key referencing collections
        let dok = Self::populate_fk_fields(
            _db,
            dok.clone(),
            coll_name,
            coll_names,
            fk_fields,
            ref_fields,
            &mut vec![],
        )
        .await;
        let dok =
            Self::populate_ref_fields(_db, dok.clone(), coll_name, coll_names, ref_fields).await;

        dok.lock().await.clone()
    }
}

#[derive(Debug, Clone)]
pub struct DateTime(bson::DateTime);
impl Deref for DateTime {
    type Target = bson::DateTime;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Default for DateTime {
    fn default() -> Self {
        Self(bson::DateTime::now())
    }
}
 impl Serialize for DateTime {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DateTime {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let inner = bson::DateTime::deserialize(deserializer)?;
        Ok(DateTime(inner))
    }
}
/*
struct DateTimeVisitor;

impl<'de> Visitor<'de> for DateTimeVisitor {
    type Value = DateTime;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "a date-time string.")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error, {
        Ok(DateTime( bson::DateTime::parse_rfc3339_str(v).expect("Failed to parse date_str.")))
    }
}

impl<'de> Deserialize<'de> for DateTime {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(DateTimeVisitor)
    }
} */

impl DateTime {
    pub fn now() -> Self {
        Self(bson::DateTime::now())
    }
    pub fn from_millis(date: i64) -> Self {
        Self(bson::DateTime::from_millis(date))
    }
}

pub fn mongo_url(offline: bool) -> String {
    let k = if offline {
        "MONGO_URL_LOCAL"
    } else {
        "MONGO_URL"
    };
    env::var(k).expect("Failed to get MONG_URL from .env.")
}

pub async fn connect_db(url: &str, db_name: &str) -> db::error::Result<()> {
    register!();
    println!("\n[{db_name}] Connecting to [{url}]...");
    let _cl = Client::with_uri_str(url).await?;
    DB.set(_cl.database(db_name)).ok();
    Ok(())
}

pub fn dbase<'a>() -> &'a Database {
    // register!();
    DB.get().unwrap()
}
