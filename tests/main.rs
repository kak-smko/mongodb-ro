use mongodb::bson::oid::ObjectId;
use mongodb::bson::{doc, DateTime};
use mongodb::{Client, Database};
use mongodb_ro::event::Boot;
use mongodb_ro::Model;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize, Deserialize, Debug, Default, Model)]
#[model(collection = "user")]
struct User {
    _id: Option<ObjectId>,
    name: String,
    #[model(asc, unique)]
    phone: String,
    #[model(desc)]
    age: u8,
    #[model(hidden, name("pswd"))]
    password: String,
    block: bool,
    updated_at: Option<DateTime>,
    created_at: Option<DateTime>,
}
impl Boot for User {
    type Req = bool;
}

#[tokio::test]
async fn test_upsert() {
    let db = get_db().await;

    User::new_model(&db, None)
        .r#where(doc! {"name":"test_upsert"})
        .upsert()
        .update(doc! {}, None)
        .await
        .unwrap();
    let user = User::new_model(&db, None)
        .r#where(doc! {"name":"test_upsert"})
        .first(None)
        .await
        .unwrap()
        .unwrap();

    assert!(user.created_at.is_some());
    assert_eq!(user.name, "test_upsert");
    User::new_model(&db, None)
        .r#where(doc! {"name":"test_upsert"})
        .delete(None)
        .await
        .unwrap();
}
#[tokio::test]
async fn save_fill() {
    let db = get_db().await;
    let user = User {
        _id: None,
        name: "smko".to_string(),
        phone: "912".to_string(),
        age: 0,
        password: "".to_string(),
        block: false,
        updated_at: None,
        created_at: None,
    };

    User::new_model(&db, None)
        .fill(user)
        .create(None)
        .await
        .unwrap();
}

#[tokio::test]
async fn save() {
    let db = get_db().await;
    let mut user_model = User::new_model(&db, None);
    user_model.name = "Smko".to_string();
    user_model.phone = "123456789".to_string();
    user_model.password = "1234".to_string();
    user_model.create(None).await.unwrap();
    let user_model = User::new_model(&db, None).distinct("name").await;
    println!("{:?}", user_model)
}

#[tokio::test]
async fn find_one() {
    let db = get_db().await;
    let user_model = User::new_model(&db, None);

    let founded = user_model
        .r#where(doc! {"name":"Smko"})
        .visible(vec!["password"])
        .first(None)
        .await
        .unwrap();
    println!("The founded object {:?} ", founded);
}

#[tokio::test]
async fn update() {
    let db = get_db().await;
    let user_model = User::new_model(&db, None);
    user_model
        .r#where(doc! {"name":"Smko"})
        .update(doc! {"age":3}, None)
        .await
        .unwrap();
    let user_model = User::new_model(&db, None);
    user_model
        .r#where(doc! {"name":"Smko"})
        .update(doc! {"$inc":{"age":1}}, None)
        .await
        .unwrap();
}
#[tokio::test]
async fn delete() {
    let db = get_db().await;
    let user_model = User::new_model(&db, None);
    user_model
        .r#where(doc! {"name":"Smko"})
        .delete(None)
        .await
        .unwrap();
}

#[tokio::test]
async fn find_and_collect() {
    let db = get_db().await;
    let user_model = User::new_model(&db, None);

    let users = user_model.get(None).await.unwrap();

    println!("The users {users:?} ")
}

#[tokio::test]
async fn transaction_with_session() {
    let db = get_db().await;
    let mut session = db.client().start_session().await.unwrap();

    // Start transaction
    session.start_transaction().await.unwrap();

    // Create a user within the transaction
    let mut user_model = User::new_model(&db, None);
    user_model.name = "TransactionUser".to_string();
    user_model.phone = "987654321".to_string();
    user_model.password = "txn_pass".to_string();
    user_model.create(Some(&mut session)).await.unwrap();

    // Verify the user exists within the transaction
    let user_model = User::new_model(&db, None);
    let user = user_model
        .r#where(doc! {"name": "TransactionUser"})
        .first(Some(&mut session))
        .await
        .unwrap();
    assert!(user.is_some(), "User should exist within transaction");

    // Commit the transaction
    session.commit_transaction().await.unwrap();

    // Verify the user exists after commit
    let user_model = User::new_model(&db, None);
    let user = user_model
        .r#where(doc! {"name": "TransactionUser"})
        .first(None)
        .await
        .unwrap();
    assert!(user.is_some(), "User should exist after commit");

    let user_model = User::new_model(&db, None);
    user_model
        .r#where(doc! {"name": "TransactionUser"})
        .delete(None)
        .await
        .unwrap();
}

async fn get_db() -> Arc<Database> {
    Arc::new(
        Client::with_uri_str("mongodb://localhost:27017")
            .await
            .expect("failed to connect")
            .database("test"),
    )
}
