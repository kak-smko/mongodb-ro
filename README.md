# MongoDB Model ORM

[![Crates.io](https://img.shields.io/crates/v/mongodb-ro)](https://crates.io/crates/mongodb-ro)
[![Documentation](https://docs.rs/mongodb-ro/badge.svg)](https://docs.rs/mongodb-ro)
[![License](https://img.shields.io/crates/l/mongodb-ro)](LICENSE-MIT)


A high-level, type-safe MongoDB model implementation with support for:
- CRUD operations
- Index management
- Field renaming and visibility control
- Timestamps
- Transactions
- Aggregation pipelines

## Features

- **Type-safe operations** - Works with Rust types that implement `Boot`, `Serialize`, and `DeserializeOwned`
- **Derive macro support** - Use `#[derive(Model)]` for easy model setup
- **Automatic index management** - Syncs indexes with model definitions
- **Field control** - Hide/show fields and rename fields in the database
- **Timestamps** - Automatic `created_at` and `updated_at` timestamps
- **Transaction support** - All operations work with MongoDB transactions
- **Query building** - Chainable methods for building complex queries

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
mongodb-ro = "1.0.0"
```

## Usage

### Database Connection Helper

```rust
async fn get_db() -> Arc<Database> {
    Arc::new(
        Client::with_uri_str("mongodb://localhost:27017")
            .await
            .expect("failed to connect")
            .database("test")
    )
}
```

### Defining a Model

```rust
use std::sync::Arc;
use mongodb::bson::{doc, DateTime};
use mongodb::bson::oid::ObjectId;
use mongodb::{Client, Database};
use mongodb_ro::Model;
use serde::{Deserialize, Serialize};
use mongodb_ro::event::Boot;

#[derive(Serialize, Deserialize, Debug, Default, Model)]
#[model(collection="user",req="bool")]
struct User {
    _id: Option<ObjectId>,
    name: String,
    #[model(asc,unique)]  // Creates ascending unique index
    phone: String,
    #[model(desc)]       // Creates descending index
    age: u8,
    #[model(hidden, name("pswd"))]  // Hidden by default and renamed in DB
    password: String,
    block: bool,
    updated_at: Option<DateTime>,
    created_at: Option<DateTime>,
}

impl Boot for User { 
    type Req = bool;  // Request context type
}
```

### Defining a Model with custom actix_web::HttpRequest

```rust
use std::sync::Arc;
use mongodb::bson::{doc, DateTime};
use mongodb::bson::oid::ObjectId;
use mongodb::{Client, Database};
use mongodb_ro::Model;
use serde::{Deserialize, Serialize};
use mongodb_ro::event::Boot;
use actix_web::HttpRequest;

#[derive(Serialize, Deserialize, Debug, Default, Model)]
#[model(collection="user",req="HttpRequest")]
struct User {
    _id: Option<ObjectId>,
    name: String,
    #[model(asc,unique)]  // Creates ascending unique index
    phone: String,
    #[model(desc)]       // Creates descending index
    age: u8,
    #[model(hidden, name("pswd"))]  // Hidden by default and renamed in DB
    password: String,
    block: bool,
    updated_at: Option<DateTime>,
    created_at: Option<DateTime>,
}

impl Boot for User { 
    type Req = HttpRequest;  // Request context type
}
```

### Basic Operations

**Create a document:**
```rust

async fn save() {
    let db = get_db().await;
    let mut user_model = User::new_model(&db, None);
    user_model.name = "Smko".to_string();
    user_model.phone = "123456789".to_string();
    user_model.password = "1234".to_string();
    user_model.create(None).await.unwrap();
}
```

**Find documents:**
```rust

async fn find_one() {
    let db = get_db().await;
    let user_model = User::new_model(&db, None);

    // Find with visible password (normally hidden)
    let user = user_model
        .r#where(doc! {"name": "Smko"})
        .visible(vec!["password"])
        .first(None)
        .await
        .unwrap();
    println!("Found user: {:?}", user);
}
```

**Update documents:**
```rust
async fn update() {
    let db = get_db().await;
    let user_model = User::new_model(&db, None);
    
    // Simple update
    user_model
        .r#where(doc! {"name": "Smko"})
        .update(doc! {"age": 3}, None)
        .await
        .unwrap();
    
    // Increment operation
    user_model
        .reset()
        .r#where(doc! {"name": "Smko"})
        .update(doc! {"$inc": {"age": 1}}, None)
        .await
        .unwrap();
}
```

**Delete documents:**
```rust
async fn delete() {
    let db = get_db().await;
    let user_model = User::new_model(&db, None);
    user_model
        .r#where(doc! {"name": "Smko"})
        .all()
        .delete(None)
        .await
        .unwrap();
}
```

### Advanced Usage

**Transactions:**
```rust
async fn transaction_with_session() {
    let db = get_db().await;
    let mut session = db.client().start_session().await.unwrap();

    // Start transaction
    session.start_transaction().await.unwrap();

    // Create within transaction
    let mut user_model = User::new_model(&db, None);
    user_model.name = "TransactionUser".to_string();
    user_model.phone = "987654321".to_string();
    user_model.password = "txn_pass".to_string();
    user_model.create(Some(&mut session)).await.unwrap();

    // Verify within transaction
    let user = User::new_model(&db, None)
        .r#where(doc! {"name": "TransactionUser"})
        .first(Some(&mut session))
        .await
        .unwrap();
    assert!(user.is_some());

    // Commit
    session.commit_transaction().await.unwrap();

    // Cleanup
    User::new_model(&db, None)
        .r#where(doc! {"name": "TransactionUser"})
        .delete(None)
        .await
        .unwrap();
}
```

**Bulk operations:**
```rust
async fn find_and_collect() {
    let db = get_db().await;
    let users = User::new_model(&db, None)
        .get(None)
        .await
        .unwrap();
    println!("All users: {:?}", users);
}
```

## Model Attributes

| Attribute    | Description                  | Example                        |
|--------------|------------------------------|--------------------------------|
| `collection` | Sets MongoDB collection name | `#[model(collection="users")]` |
| `req`        | Request context type         | `#[model(req="bool")]`         |


## Field Attributes

| Attribute  | Description               | Example                    |
|------------|---------------------------|----------------------------|
| `name`     | Renames field in database | `#[model(name="db_name")]` |
| `hidden`   | Hides field by default    | `#[model(hidden)]`         |
| `sphere2d` | Creates sphere2d index    | `#[model(sphere2d)]`       |
| `text`     | Creates text index        | `#[model(text="en")]`      |
| `asc`      | Creates ascending index   | `#[model(asc)]`            |
| `desc`     | Creates descending index  | `#[model(desc)]`           |
| `unique`   | Creates unique index      | `#[model(unique)]`         |




## Contributing

Contributions are welcome! Please open an issue or submit a PR for:
- New features
- Performance improvements
- Bug fixes


## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE) at your option.