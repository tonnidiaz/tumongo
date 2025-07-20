use std::str::FromStr;

// Bring in the ctor crate
pub use ctor;
use db::{
    ClientSession, Database,
    bson::{Document, doc, oid::ObjectId},
};
use futures_util::TryStreamExt;
pub use tumongo_derive::*;
pub use tumongo_utils::*;
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

                let on_delete = item.on_delete.as_ref().and_then(|s| OnDelete::from_str(s).ok());
                let mut delete = false;

                if let Some(on_delete) = on_delete{
                    match on_delete { 
                        OnDelete::Null =>{
                            println!("\nClearing {} from {}...", field_name, item.coll);
                            // item: tri_order, field_name: order_a
                            let collection = db.collection::<Document>(&item.coll);
                            let tx = collection.update_many(doc! { field_name: Some(id) }, doc! {
                                "$set": doc! { field_name: None::<ObjectId> }
                            }).session(&mut *sess).await?;
                            println!("\n{} {} cleared from {} collection", tx.modified_count, field_name, item.coll);
                        }
                        OnDelete::Cascade => delete = true
                    };
                }

                if !delete{
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
}
