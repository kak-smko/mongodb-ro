use futures_util::StreamExt;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{doc, Bson, DateTime};
use mongodb::{Client, Database};
use mongodb_ro::event::Boot;
use mongodb_ro::model::Model;
use mongodb_ro::Model;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Default, Model, PartialEq)]
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

async fn get_db() -> Database {
    Client::with_uri_str("mongodb://localhost:27017")
        .await
        .expect("failed to connect")
        .database("test")
}

async fn cleanup_users(db: &Database) {
    User::new_model(db).collection().drop().await.unwrap();
}

async fn setup_test_user<'a>(db: &Database, name: &str, phone: &str, age: u8) -> Model<'a, User> {
    let mut user = User::new_model(db);
    user.name = name.to_string();
    user.phone = phone.to_string();
    user.age = age;
    user.create().await.unwrap();
    user
}

#[tokio::test]
async fn test_all() {
    test_count_documents().await;
    test_upsert().await;
    test_save_fill().await;
    test_create_many().await;
    test_save_and_retrieve().await;
    test_find_one_with_visibility().await;
    test_cursor_iteration().await;
    test_update_operations().await;
    test_delete_operation().await;
    test_find_and_collect_multiple().await;
    test_transaction_with_session().await;
    test_select().await;
}

async fn test_select() {
    let db = get_db().await;
    cleanup_users(&db).await;

    let mut user_model = User::new_model(&db);
    user_model.name = "test_visibility".to_string();
    user_model.phone = "555555555".to_string();
    user_model.password = "secret".to_string();
    user_model.create().await.unwrap();


    let hidden_user = User::new_model(&db)
        .select(doc!{"name":1})
        .r#where(doc! {"name": "test_visibility"})
        .first()
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        hidden_user.phone, "",
        "phone should be hidden"
    );
    let hidden_user = User::new_model(&db)
        .select(doc!{"name":1})
        .r#where(doc! {"name": "test_visibility"})
        .first_doc()
        .await
        .unwrap()
        .unwrap();

    assert!(
        hidden_user.get("phone").is_none(),
        "phone should be hidden"
    );
    cleanup_users(&db).await;
}
async fn test_count_documents() {
    let db = get_db().await;
    cleanup_users(&db).await;

    // Insert test data
    for i in 0..5 {
        setup_test_user(&db, "test_count_user", &format!("12345678{i}"), i as u8).await;
    }

    // Test basic count
    let total_count = User::new_model(&db)
        .r#where(doc! {"name": "test_count_user"})
        .count_documents()
        .await
        .unwrap();
    assert_eq!(total_count, 5, "Should count all matching documents");

    // Test count with limit
    let limited_count = User::new_model(&db)
        .r#where(doc! {"name": "test_count_user"})
        .limit(3)
        .count_documents()
        .await
        .unwrap();
    assert_eq!(limited_count, 3, "Should respect limit");

    // Test count with skip
    let skipped_count = User::new_model(&db)
        .r#where(doc! {"name": "test_count_user"})
        .skip(2)
        .count_documents()
        .await
        .unwrap();
    assert_eq!(skipped_count, 3, "Should respect skip");

    // Test count with skip and limit
    let skip_limit_count = User::new_model(&db)
        .r#where(doc! {"name": "test_count_user"})
        .skip(1)
        .limit(2)
        .count_documents()
        .await
        .unwrap();
    assert_eq!(skip_limit_count, 2, "Should respect both skip and limit");

    // Test count with age filter
    let age_filter_count = User::new_model(&db)
        .r#where(doc! {
            "name": "test_count_user",
            "age": { "$gt": 2 }
        })
        .count_documents()
        .await
        .unwrap();
    assert_eq!(
        age_filter_count, 2,
        "Should count only documents matching age filter"
    );

    cleanup_users(&db).await;
}

async fn test_upsert() {
    let db = get_db().await;
    cleanup_users(&db).await;

    // First upsert (create)
    User::new_model(&db)
        .set_request(true)
        .r#where(doc! {"name": "test_upsert"})
        .upsert()
        .update(doc! {"$set": {"phone": "123456789"}})
        .await
        .unwrap();

    let user = User::new_model(&db)
        .r#where(doc! {"name": "test_upsert"})
        .first()
        .await
        .unwrap()
        .unwrap();

    assert!(user.created_at.is_some());
    assert_eq!(user.name, "test_upsert");
    assert_eq!(user.phone, "123456789");

    // Second upsert (update)
    User::new_model(&db)
        .set_request(true)
        .r#where(doc! {"name": "test_upsert"})
        .upsert()
        .update(doc! {"$set": {"phone": "987654321"}})
        .await
        .unwrap();

    let updated_user = User::new_model(&db)
        .r#where(doc! {"name": "test_upsert"})
        .first()
        .await
        .unwrap()
        .unwrap();

    assert_eq!(updated_user.phone, "987654321");
    assert_eq!(
        user._id, updated_user._id,
        "Should update existing document"
    );

    cleanup_users(&db).await;
}

async fn test_save_fill() {
    let db = get_db().await;
    cleanup_users(&db).await;

    let user = User {
        _id: None,
        name: "test_save_fill".to_string(),
        phone: "912".to_string(),
        age: 25,
        password: "".to_string(),
        block: false,
        updated_at: None,
        created_at: None,
    };

    User::new_model(&db).fill(user).create().await.unwrap();

    let fetched_user = User::new_model(&db)
        .r#where(doc! {"name": "test_save_fill"})
        .first()
        .await
        .unwrap()
        .unwrap();

    assert_eq!("test_save_fill".to_string(), fetched_user.name);
    assert_eq!("912".to_string(), fetched_user.phone);
    assert_eq!(25, fetched_user.age);
    assert!(fetched_user.created_at.is_some());

    cleanup_users(&db).await;
}
async fn test_create_many() {
    let db = get_db().await;
    cleanup_users(&db).await;

    User::new_model(&db).create_many_doc(vec![doc!{"name":"test1","phone":"123"},doc!{"name":"test2","phone":"124"}]).await.unwrap();

    let fetched_user = User::new_model(&db)
        .count_documents()
        .await
        .unwrap();


    assert_eq!(2,fetched_user);

    let fetched_user = User::new_model(&db)
        .r#where(doc! {"name": "test1"})
        .first()
        .await
        .unwrap()
        .unwrap();

    assert_eq!("test1".to_string(), fetched_user.name);
    assert_eq!("123".to_string(), fetched_user.phone);

    cleanup_users(&db).await;
}

async fn test_save_and_retrieve() {
    let db = get_db().await;
    cleanup_users(&db).await;

    let mut user_model = User::new_model(&db);
    user_model.name = "test_save".to_string();
    user_model.phone = "123456789".to_string();
    user_model.password = "1234".to_string();
    user_model.age = 30;
    user_model.create().await.unwrap();

    let distinct_names = User::new_model(&db).distinct("name").await.unwrap();
    assert!(distinct_names.contains(&Bson::String("test_save".to_string())));

    cleanup_users(&db).await;
}

async fn test_find_one_with_visibility() {
    let db = get_db().await;
    cleanup_users(&db).await;

    let mut user_model = User::new_model(&db);
    user_model.name = "test_visibility".to_string();
    user_model.phone = "555555555".to_string();
    user_model.password = "secret".to_string();
    user_model.create().await.unwrap();

    let visible_user = User::new_model(&db)
        .r#where(doc! {"name": "test_visibility"})
        .visible(vec!["password"])
        .first()
        .await
        .unwrap()
        .unwrap();

    assert_eq!(visible_user.password, "secret");

    let hidden_user = User::new_model(&db)
        .r#where(doc! {"name": "test_visibility"})
        .first()
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        hidden_user.password, "",
        "Password should be hidden by default"
    );

    cleanup_users(&db).await;
}

async fn test_cursor_iteration() {
    let db = get_db().await;
    cleanup_users(&db).await;

    for i in 0..10 {
        User::new_model(&db)
            .fill(User {
                _id: None,
                name: format!("test_cursor_user_{}", i),
                phone: i.to_string(),
                age: i as u8,
                password: "".to_string(),
                block: false,
                updated_at: None,
                created_at: None,
            })
            .create()
            .await
            .unwrap();
    }

    let mut cursor = User::new_model(&db).cursor().await.unwrap();

    let mut count = 0;
    while let Some(doc) = cursor.next().await {
        let doc = doc.unwrap();
        assert!(
            doc.get_str("name")
                .unwrap()
                .starts_with("test_cursor_user_")
        );
        count += 1;
    }

    assert_eq!(count, 10);

    cleanup_users(&db).await;
}

async fn test_update_operations() {
    let db = get_db().await;
    cleanup_users(&db).await;

    let mut user_model = User::new_model(&db);
    user_model.name = "test_update".to_string();
    user_model.phone = "111111111".to_string();
    user_model.age = 20;
    user_model.create().await.unwrap();

    // Simple update
    User::new_model(&db)
        .r#where(doc! {"name": "test_update"})
        .update(doc! {"$set": {"age": 25}})
        .await
        .unwrap();

    let updated_user = User::new_model(&db)
        .r#where(doc! {"name": "test_update"})
        .first()
        .await
        .unwrap()
        .unwrap();

    assert_eq!(updated_user.age, 25);

    // Increment operation
    User::new_model(&db)
        .r#where(doc! {"name": "test_update"})
        .update(doc! {"$inc": {"age": 1}})
        .await
        .unwrap();

    let incremented_user = User::new_model(&db)
        .r#where(doc! {"name": "test_update"})
        .first()
        .await
        .unwrap()
        .unwrap();

    assert_eq!(incremented_user.age, 26);
    assert!(incremented_user.updated_at.is_some());

    cleanup_users(&db).await;
}

async fn test_delete_operation() {
    let db = get_db().await;
    cleanup_users(&db).await;

    let mut user_model = User::new_model(&db);
    user_model.name = "test_delete".to_string();
    user_model.phone = "222222222".to_string();
    user_model.create().await.unwrap();

    // Verify exists
    let exists_before = User::new_model(&db)
        .r#where(doc! {"name": "test_delete"})
        .first()
        .await
        .unwrap();
    assert!(exists_before.is_some());

    // Delete
    User::new_model(&db)
        .r#where(doc! {"name": "test_delete"})
        .delete()
        .await
        .unwrap();

    // Verify deleted
    let exists_after = User::new_model(&db)
        .r#where(doc! {"name": "test_delete"})
        .first()
        .await
        .unwrap();
    assert!(exists_after.is_none());
}

async fn test_find_and_collect_multiple() {
    let db = get_db().await;
    cleanup_users(&db).await;

    // Create multiple users
    for i in 0..5 {
        setup_test_user(&db, "test_collect", &format!("33333333{i}"), i as u8 + 20).await;
    }

    let users = User::new_model(&db)
        .r#where(doc! {"name": "test_collect"})
        .get()
        .await
        .unwrap();

    assert_eq!(users.len(), 5);
    for user in users {
        assert!(user.age >= 20 && user.age < 25);
    }

    cleanup_users(&db).await;
}

async fn test_transaction_with_session() {
    let db = get_db().await;
    cleanup_users(&db).await;

    let mut session = db.client().start_session().await.unwrap();

    // Start transaction
    session.start_transaction().await.unwrap();

    // Create a user within the transaction
    let mut user_model = User::new_model(&db);
    user_model.name = "test_transaction".to_string();
    user_model.phone = "444444444".to_string();
    user_model.password = "txn_pass".to_string();
    user_model.create_with_session(&mut session).await.unwrap();

    // Verify the user exists within the transaction
    let user_in_txn = User::new_model(&db)
        .r#where(doc! {"name": "test_transaction"})
        .first_with_session(&mut session)
        .await
        .unwrap();
    assert!(
        user_in_txn.is_some(),
        "User should exist within transaction"
    );

    // Verify the user doesn't exist outside transaction
    let user_outside_txn = User::new_model(&db)
        .r#where(doc! {"name": "test_transaction"})
        .first()
        .await
        .unwrap();
    assert!(
        user_outside_txn.is_none(),
        "User should not exist outside transaction before commit"
    );

    // Commit the transaction
    session.commit_transaction().await.unwrap();

    // Verify the user exists after commit
    let user_after_commit = User::new_model(&db)
        .r#where(doc! {"name": "test_transaction"})
        .first()
        .await
        .unwrap();
    assert!(
        user_after_commit.is_some(),
        "User should exist after commit"
    );

    // Cleanup
    User::new_model(&db)
        .r#where(doc! {"name": "test_transaction"})
        .delete()
        .await
        .unwrap();
}
