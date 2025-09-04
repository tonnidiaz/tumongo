use serde::{Deserialize, Serialize};
use tumongo::{db::bson::{oid::ObjectId, doc}, DateTime, TumongoModel};

#[derive(TumongoModel, Debug, Serialize, Deserialize, Default)]
#[tumongo(coll_name = "persons")] 
struct Person {
    pub id: Option<ObjectId>,
    #[tumongo(unique)]
    pub name: String,
    #[tumongo(unique_if_same = "company_id")]
    pub alias: String,
    #[tumongo(fk, coll = "company")]
    pub company_id: i64,
    #[tumongo(reff, coll = "cars")]
    pub cars: Vec<ObjectId>,
    pub created_at: DateTime,
    pub updated_at: tumongo::DateTime,
}
 
#[derive(TumongoModel, Serialize, Deserialize)]
#[tumongo(coll_name = "cars")] 
struct Car{
    pub id: Option<ObjectId>,
    #[tumongo(fk, coll="company")]
    pub company_id: ObjectId,
    #[tumongo(fk, coll="persons")]
    pub owner_id: ObjectId, 
    pub created_at: DateTime,
    pub updated_at: tumongo::DateTime,
}
 
#[tokio::main]
async fn main() { 
    println!("Hello, world!"); 
    tumongo::register!(); 
    let conn_str = "mongodb://localhost:27017/?readPreference=primary&directConnection=true&ssl=false";
    let db = tumongo::db::Client::with_uri_str(conn_str).await.expect("Failed to connect db").database("tutest");
    let mut p = Person{
        name: "Tonni Diaz_1756952750__updated".to_string(),//format!("Tonni Diaz_{}", Local::now().timestamp()),
        company_id: 10,
        alias: "tonics2".to_owned(),
        ..Default::default()};
    // let mut p = Person::find_one(&db, doc!{"_id": Some(ObjectId::from_str("68b8f8ab65e56ef6c22a021c").unwrap()) }).await.unwrap();
    println!("{p:#?}");
    p.save(&db).await.expect("Failed to insert");
    println!("inserted: {}", p.id.unwrap());
    println!("{p:#?}");

    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
    println!("Updating...");
    p.name = p.name;//format!("{}__updated", p.name);
    p.save(&db).await.expect("Failed to update");
    println!("{p:#?}");


}
 