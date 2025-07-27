use crate::column::ColumnAttr;
use crate::event::Boot;
use futures_util::StreamExt;
use log::error;
use mongodb::action::EstimatedDocumentCount;
use mongodb::bson::{doc, to_document, Document};
use mongodb::bson::{Bson, DateTime};
use mongodb::error::{Error, Result};
use mongodb::options::{CountOptions, IndexOptions};
use mongodb::results::InsertOneResult;
use mongodb::{bson, ClientSession, Collection, Database, IndexModel};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt::Debug;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

pub type MongodbResult<T> = Result<T>;

#[derive(Debug, Default, Clone)]
struct QueryBuilder {
    pub r#where: Vec<Document>,
    pub all: bool,
    pub upsert: bool,
    pub select: Option<Document>,
    pub sort: Document,
    pub skip: u32,
    pub limit: u32,
    pub visible_fields: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Model<'a, M>
where
    M: Boot,
{
    inner: Box<M>,
    #[serde(skip_serializing)]
    req: Option<M::Req>,
    #[serde(skip)]
    db: Database,
    #[serde(skip)]
    collection_name: &'a str,
    #[serde(skip)]
    add_times: bool,
    #[serde(skip)]
    columns: HashMap<&'a str, ColumnAttr>,
    #[serde(skip)]
    query_builder: QueryBuilder,
}

impl<'a, T: 'a + Boot> Deref for Model<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a, T: 'a + Boot> DerefMut for Model<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<'a, M> Model<'a, M>
where
    M: Boot,
    M: Default,
    M: Serialize,
    M: DeserializeOwned,
    M: Send,
    M: Sync,
    M: Unpin,
{
    pub fn new(
        db: &Database,
        collection_name: &'a str,
        columns: &'a str,
        add_times: bool,
    ) -> Model<'a, M> {
        let columns = serde_json::from_str(columns).unwrap();

        let model = Model {
            inner: Box::<M>::default(),
            req: None,
            db: db.clone(),
            collection_name,
            columns,
            add_times,
            query_builder: Default::default(),
        };

        model
    }

    /// Set Request to model
    pub fn set_request(mut self, req: M::Req) -> Model<'a, M> {
        self.req = Some(req);
        self
    }

    /// add lazy column to model
    pub fn add_columns(&mut self, names: Vec<&'a str>) {
        for name in names {
            self.columns.insert(
                name,
                ColumnAttr {
                    asc: false,
                    desc: false,
                    unique: false,
                    sphere2d: false,
                    text: None,
                    hidden: false,
                    name: Some(name.to_string()),
                },
            );
        }
    }

    /// Gets the collection name
    pub fn collection_name(&self) -> &'a str {
        self.collection_name
    }

    /// Gets a handle to the MongoDB collection
    pub fn collection(&self) -> Collection<M> {
        self.db.collection::<M>(self.collection_name)
    }
    /// Changes the collection name for this model
    pub fn set_collection(mut self, name: &'a str) -> Model<'a, M> {
        self.collection_name = name;
        self
    }

    /// Registers indexes based on column attributes
    ///
    /// This will:
    /// 1. Check existing indexes
    /// 2. Remove indexes for fields that no longer exist in the model
    /// 3. Create new indexes for fields marked as indexes in column attributes
    pub async fn register_indexes(&self) {
        let coll = self.db.collection::<M>(self.collection_name);
        let previous_indexes = coll.list_indexes().await;
        let mut attrs = vec![];
        for (name, attr) in &self.columns {
            if attr.is_index() {
                attrs.push(name)
            }
        }

        let mut keys_to_remove = Vec::new();
        if previous_indexes.is_ok() {
            let foreach_future = previous_indexes.unwrap().for_each(|pr| {
                match pr {
                    Ok(index_model) => {
                        index_model.keys.iter().for_each(|key| {
                            if key.0 != "_id" {
                                if let Some(pos) = attrs.iter().position(|k| k == &key.0) {
                                    // means attribute exists in struct and database and not need to create it
                                    attrs.remove(pos);
                                } else if let Some(rw) = &index_model.options {
                                    // means the attribute must remove because not exists in struct
                                    match rw.default_language {
                                        None => keys_to_remove.push(rw.name.clone()),
                                        Some(_) => match &rw.name {
                                            None => keys_to_remove.push(rw.name.clone()),
                                            Some(name) => {
                                                if let Some(pos) =
                                                    attrs.iter().position(|k| k == &name)
                                                {
                                                    attrs.remove(pos);
                                                } else {
                                                    keys_to_remove.push(rw.name.clone())
                                                }
                                            }
                                        },
                                    }
                                }
                            }
                        });
                    }
                    Err(error) => {
                        error!("Can't unpack index model {error}");
                    }
                }
                futures::future::ready(())
            });
            foreach_future.await;
        }

        let attrs = attrs
            .iter()
            .map(|name| {
                let key = name.to_string();
                let attr = &self.columns.get(key.as_str()).unwrap();

                if let Some(lang) = &attr.text {
                    let opts = IndexOptions::builder()
                        .unique(attr.unique)
                        .name(key.clone())
                        .default_language(lang.to_string())
                        .build();
                    IndexModel::builder()
                        .keys(doc! {
                            key : "text"
                        })
                        .options(opts)
                        .build()
                } else if attr.sphere2d {
                    let opts = IndexOptions::builder().unique(attr.unique).build();
                    IndexModel::builder()
                        .keys(doc! { key: "2dsphere" })
                        .options(opts)
                        .build()
                } else {
                    let sort = if attr.desc { -1 } else { 1 };
                    let opts = IndexOptions::builder().unique(attr.unique).build();

                    IndexModel::builder()
                        .keys(doc! {
                            key : sort
                        })
                        .options(opts)
                        .build()
                }
            })
            .collect::<Vec<IndexModel>>();

        for name in keys_to_remove {
            let key = name.as_ref().unwrap();
            let _ = coll.drop_index(key).await;
        }
        if !attrs.is_empty() {
            let result = coll.create_indexes(attrs).await;
            if let Err(error) = result {
                error!("Can't create indexes : {:?}", error);
            }
        }
    }

    /// Reset all filters
    pub fn reset(mut self) -> Model<'a, M> {
        self.query_builder = Default::default();
        self
    }
    /// Adds a filter condition to the query
    pub fn r#where(mut self, data: Document) -> Model<'a, M> {
        self.query_builder.r#where.push(data);
        self
    }
    /// Sets the number of documents to skip
    pub fn skip(mut self, count: u32) -> Model<'a, M> {
        self.query_builder.skip = count;
        self
    }
    /// Gets distinct values for a field
    pub async fn distinct(&self, name: &str) -> Result<Vec<Bson>> {
        let whr = &self.query_builder.r#where;
        let filter = if whr.is_empty() {
            doc! {}
        } else {
            doc! {"$and":whr}
        };
        let collection = self.db.collection::<Document>(self.collection_name);
        collection.distinct(&name, filter).await
    }
    /// Sets the maximum number of documents to return
    pub fn limit(mut self, count: u32) -> Model<'a, M> {
        self.query_builder.limit = count;
        self
    }
    /// Sets the sort order
    pub fn sort(mut self, data: Document) -> Model<'a, M> {
        self.query_builder.sort = data;
        self
    }
    /// Sets whether to affect all matching documents (for update/delete)
    pub fn all(mut self) -> Model<'a, M> {
        self.query_builder.all = true;
        self
    }
    /// Sets the projection (field selection)
    pub fn select(mut self, data: Document) -> Model<'a, M> {
        self.query_builder.select = Some(data);
        self
    }
    /// Sets which fields should be visible (overrides hidden fields)
    pub fn visible(mut self, data: Vec<&str>) -> Model<'a, M> {
        self.query_builder.visible_fields = data.iter().map(|a| a.to_string()).collect();
        self
    }
    /// Sets whether to upsert on update
    pub fn upsert(mut self) -> Model<'a, M> {
        self.query_builder.upsert = true;
        self
    }

    /// Get Documents count with filters
    pub async fn count_documents(self) -> Result<u64> {
        let whr = &self.query_builder.r#where;
        let collection = self.db.collection::<Document>(self.collection_name);
        let filter = if whr.is_empty() {
            doc! {}
        } else {
            doc! { "$and": whr }
        };

        let options = CountOptions::builder()
            .skip(if self.query_builder.skip > 0 {
                Some(self.query_builder.skip as u64)
            } else {
                None
            })
            .limit(if self.query_builder.limit > 0 {
                Some(self.query_builder.limit as u64)
            } else {
                None
            })
            .build();

        collection
            .count_documents(filter)
            .with_options(options)
            .await
    }

    /// Creates a new document in the collection
    ///
    /// # Arguments
    /// * `session` - Optional MongoDB transaction session
    ///
    /// # Notes
    /// - Automatically adds timestamps if configured
    pub async fn create(&self, session: Option<&mut ClientSession>) -> Result<InsertOneResult> {
        let mut data = self.inner_to_doc()?;
        if data.get_object_id("_id").is_err() {
            data.remove("_id");
        }
        if self.add_times {
            if !data.contains_key("updated_at") || !data.get_datetime("updated_at").is_ok() {
                data.insert("updated_at", DateTime::now());
            }
            if !data.contains_key("created_at") || !data.get_datetime("created_at").is_ok() {
                data.insert("created_at", DateTime::now());
            }
        }
        match session {
            None => {
                let r = self
                    .db
                    .collection(self.collection_name)
                    .insert_one(data.clone())
                    .await;
                if r.is_ok() {
                    self.finish(&self.req, "create", Document::new(), data, None)
                        .await;
                }
                r
            }
            Some(s) => {
                let r = self
                    .db
                    .collection(self.collection_name)
                    .insert_one(data.clone())
                    .session(&mut *s)
                    .await;
                if r.is_ok() {
                    self.finish(&self.req, "create", Document::new(), data, Some(s))
                        .await;
                }
                r
            }
        }
    }

    /// Creates a new document from raw BSON
    pub async fn create_doc(
        &self,
        data: Document,
        session: Option<&mut ClientSession>,
    ) -> Result<InsertOneResult> {
        let mut data = data;

        if self.add_times {
            if !data.contains_key("updated_at") || !data.get_datetime("updated_at").is_ok() {
                data.insert("updated_at", DateTime::now());
            }
            if !data.contains_key("created_at") || !data.get_datetime("created_at").is_ok() {
                data.insert("created_at", DateTime::now());
            }
        }
        match session {
            None => {
                let r = self
                    .db
                    .collection(self.collection_name)
                    .insert_one(data.clone())
                    .await;
                if r.is_ok() {
                    self.finish(&self.req, "create", Document::new(), data, None)
                        .await;
                }
                r
            }
            Some(s) => {
                let r = self
                    .db
                    .collection(self.collection_name)
                    .insert_one(data.clone())
                    .session(&mut *s)
                    .await;
                if r.is_ok() {
                    self.finish(&self.req, "create", Document::new(), data, Some(s))
                        .await;
                }
                r
            }
        }
    }

    /// Updates documents in the collection
    ///
    /// # Arguments
    /// * `data` - Update operations
    /// * `session` - Optional MongoDB transaction session
    ///
    /// # Notes
    /// - Automatically adds updated_at timestamp if configured
    /// - Handles both single and multi-document updates based on `all()` setting
    /// - Supports upsert if configured
    pub async fn update(
        &self,
        data: Document,
        session: Option<&mut ClientSession>,
    ) -> Result<Document> {
        let mut data = data;
        let mut is_opt = false;
        for (a, _) in data.iter() {
            if a.starts_with("$") {
                is_opt = true;
            }
        }

        self.rename_field(&mut data, is_opt);
        if !is_opt {
            data = doc! {"$set":data};
        }
        if self.add_times {
            if !data.contains_key("$set") {
                data.insert("$set", doc! {});
            }
            let set = data.get_mut("$set").unwrap().as_document_mut().unwrap();
            set.insert("updated_at", DateTime::now());
        }

        if self.query_builder.upsert {
            if self.add_times {
                if !data.contains_key("$setOnInsert") {
                    data.insert("$setOnInsert", doc! {});
                }
                let set = data
                    .get_mut("$setOnInsert")
                    .unwrap()
                    .as_document_mut()
                    .unwrap();
                set.insert("created_at", DateTime::now());
            }
        }
        let whr = &self.query_builder.r#where;
        if whr.is_empty() {
            return Err(Error::from(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "where not set.",
            )));
        }
        let filter = doc! {"$and":whr};

        match session {
            None => {
                let r = self.db.collection::<Document>(self.collection_name);

                if self.query_builder.all {
                    let r = r
                        .update_many(filter, data.clone())
                        .upsert(self.query_builder.upsert)
                        .await;
                    match r {
                        Ok(old) => {
                            let res = doc! {"modified_count":old.modified_count.to_string()};
                            self.finish(&self.req, "update_many", res.clone(), data, None)
                                .await;
                            Ok(res)
                        }
                        Err(e) => Err(e),
                    }
                } else {
                    let r = r
                        .find_one_and_update(filter, data.clone())
                        .upsert(self.query_builder.upsert)
                        .sort(self.query_builder.sort.clone())
                        .await;
                    match r {
                        Ok(old) => {
                            let res = old.unwrap_or(Document::new());
                            self.finish(&self.req, "update", res.clone(), data, None)
                                .await;
                            Ok(res)
                        }
                        Err(e) => Err(e),
                    }
                }
            }
            Some(s) => {
                let r = self.db.collection::<Document>(self.collection_name);
                if self.query_builder.all {
                    let r = r
                        .update_many(filter, data.clone())
                        .upsert(self.query_builder.upsert)
                        .session(&mut *s)
                        .await;
                    match r {
                        Ok(old) => {
                            let res = doc! {"modified_count":old.modified_count.to_string()};
                            self.finish(&self.req, "update_many", res.clone(), data, Some(s))
                                .await;
                            Ok(res)
                        }
                        Err(e) => Err(e),
                    }
                } else {
                    let r = r
                        .find_one_and_update(filter, data.clone())
                        .upsert(self.query_builder.upsert)
                        .sort(self.query_builder.sort.clone())
                        .session(&mut *s)
                        .await;
                    match r {
                        Ok(old) => {
                            let res = old.unwrap_or(Document::new());
                            self.finish(&self.req, "update", res.clone(), data, Some(s))
                                .await;
                            Ok(res)
                        }
                        Err(e) => Err(e),
                    }
                }
            }
        }
    }

    /// Deletes documents from the collection
    ///
    /// # Arguments
    /// * `session` - Optional MongoDB transaction session
    ///
    /// # Notes
    /// - Handles both single and multi-document deletes based on `all()` setting
    pub async fn delete(&self, session: Option<&mut ClientSession>) -> Result<Document> {
        let whr = &self.query_builder.r#where;
        if whr.is_empty() {
            return Err(Error::from(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "where not set.",
            )));
        }
        let filter = doc! {"$and":whr};

        match session {
            None => {
                let r = self.db.collection::<Document>(self.collection_name);
                if self.query_builder.all {
                    let r = r.delete_many(filter).await;
                    match r {
                        Ok(old) => {
                            let res = doc! {"deleted_count":old.deleted_count.to_string()};
                            self.finish(&self.req, "delete_many", res.clone(), doc! {}, None)
                                .await;
                            Ok(res)
                        }
                        Err(e) => Err(e),
                    }
                } else {
                    let r = r
                        .find_one_and_delete(filter)
                        .sort(self.query_builder.sort.clone())
                        .await;
                    match r {
                        Ok(old) => {
                            let res = old.unwrap_or(Document::new());
                            self.finish(&self.req, "delete", res.clone(), doc! {}, None)
                                .await;
                            Ok(res)
                        }
                        Err(e) => Err(e),
                    }
                }
            }
            Some(s) => {
                let r = self.db.collection::<Document>(self.collection_name);
                if self.query_builder.all {
                    let r = r.delete_many(filter).session(&mut *s).await;
                    match r {
                        Ok(old) => {
                            let res = doc! {"deleted_count":old.deleted_count.to_string()};
                            self.finish(&self.req, "delete_many", res.clone(), doc! {}, Some(s))
                                .await;
                            Ok(res)
                        }
                        Err(e) => Err(e),
                    }
                } else {
                    let r = r
                        .find_one_and_delete(filter)
                        .sort(self.query_builder.sort.clone())
                        .session(&mut *s)
                        .await;
                    match r {
                        Ok(old) => {
                            let res = old.unwrap_or(Document::new());
                            self.finish(&self.req, "delete", res.clone(), doc! {}, Some(s))
                                .await;
                            Ok(res)
                        }
                        Err(e) => Err(e),
                    }
                }
            }
        }
    }

    /// Queries documents from the collection
    ///
    /// # Arguments
    /// * `session` - Optional MongoDB transaction session
    ///
    /// # Notes
    /// - Respects skip/limit/sort/select settings
    /// - Filters out hidden fields unless explicitly made visible
    pub async fn get(&self, session: Option<&mut ClientSession>) -> Result<Vec<M>> {
        let whr = &self.query_builder.r#where;
        let filter = if whr.is_empty() {
            doc! {}
        } else {
            doc! {"$and":whr}
        };
        let hidden_fields = self.hidden_fields();
        let collection = self.db.collection::<Document>(self.collection_name);
        let mut find = collection.find(filter);
        find = find.sort(self.query_builder.sort.clone());

        if self.query_builder.skip > 0 {
            find = find.skip(self.query_builder.skip as u64);
        }
        if self.query_builder.limit > 0 {
            find = find.limit(self.query_builder.limit as i64);
        }
        if let Some(select) = self.query_builder.select.clone() {
            find = find.projection(select);
        }

        let mut r = vec![];
        match session {
            None => {
                let mut cursor = find.await?;
                while let Some(d) = cursor.next().await {
                    r.push(self.clear(self.cast(d?, &self.req), &hidden_fields))
                }
                Ok(r)
            }
            Some(s) => {
                let mut cursor = find.session(&mut *s).await?;
                while let Some(d) = cursor.next(&mut *s).await {
                    r.push(self.clear(self.cast(d?, &self.req), &hidden_fields))
                }
                Ok(r)
            }
        }
    }

    /// Gets the first matching document
    pub async fn first(&mut self, session: Option<&mut ClientSession>) -> Result<Option<M>> {
        self.query_builder.limit = 1;
        let r = self.get(session).await?;
        for item in r {
            return Ok(Some(item));
        }
        Ok(None)
    }

    /// Runs an aggregation pipeline
    pub async fn aggregate(
        &mut self,
        pipeline: impl IntoIterator<Item = Document>,
        session: Option<&mut ClientSession>,
    ) -> Result<Vec<M>> {
        let collection = self.db.collection::<Document>(self.collection_name);
        let res = collection.aggregate(pipeline);
        let hidden_fields = self.hidden_fields();
        let mut r = vec![];
        match session {
            None => {
                let mut cursor = res.await?;
                while let Some(d) = cursor.next().await {
                    r.push(self.clear(self.cast(d?, &self.req), &hidden_fields))
                }
                Ok(r)
            }
            Some(s) => {
                let mut cursor = res.session(&mut *s).await?;
                while let Some(d) = cursor.next(&mut *s).await {
                    r.push(self.clear(self.cast(d?, &self.req), &hidden_fields))
                }
                Ok(r)
            }
        }
    }

    /// Queries documents and returns raw BSON
    pub async fn get_doc(&self, session: Option<&mut ClientSession>) -> Result<Vec<Document>> {
        let whr = &self.query_builder.r#where;
        let filter = if whr.is_empty() {
            doc! {}
        } else {
            doc! {"$and":whr}
        };
        let collection = self.db.collection::<Document>(self.collection_name);
        let mut find = collection.find(filter);
        find = find.sort(self.query_builder.sort.clone());

        if self.query_builder.skip > 0 {
            find = find.skip(self.query_builder.skip as u64);
        }
        if self.query_builder.limit > 0 {
            find = find.limit(self.query_builder.limit as i64);
        }
        if let Some(select) = self.query_builder.select.clone() {
            find = find.projection(select);
        }

        let mut r = vec![];
        match session {
            None => {
                let mut cursor = find.await?;
                while let Some(d) = cursor.next().await {
                    r.push(self.cast(d?, &self.req))
                }
                Ok(r)
            }
            Some(s) => {
                let mut cursor = find.session(&mut *s).await?;
                while let Some(d) = cursor.next(&mut *s).await {
                    r.push(self.cast(d?, &self.req))
                }
                Ok(r)
            }
        }
    }

    /// Queries documents and returns first raw BSON
    pub async fn first_doc(
        &mut self,
        session: Option<&mut ClientSession>,
    ) -> Result<Option<Document>> {
        self.query_builder.limit = 1;
        let r = self.get_doc(session).await?;
        for item in r {
            return Ok(Some(item));
        }
        Ok(None)
    }

    /// Runs an aggregation pipeline and returns raw BSON
    pub async fn aggregate_doc(
        &mut self,
        pipeline: impl IntoIterator<Item = Document>,
        session: Option<&mut ClientSession>,
    ) -> Result<Vec<Document>> {
        let collection = self.db.collection::<Document>(self.collection_name);
        let res = collection.aggregate(pipeline);
        let mut r = vec![];
        match session {
            None => {
                let mut cursor = res.await?;
                while let Some(d) = cursor.next().await {
                    r.push(self.cast(d?, &self.req))
                }
                Ok(r)
            }
            Some(s) => {
                let mut cursor = res.session(&mut *s).await?;
                while let Some(d) = cursor.next(&mut *s).await {
                    r.push(self.cast(d?, &self.req))
                }
                Ok(r)
            }
        }
    }

    fn hidden_fields(&self) -> Vec<String> {
        let mut r = vec![];
        for (name, attr) in &self.columns {
            if attr.hidden && !self.query_builder.visible_fields.contains(&name.to_string()) {
                r.push(name.to_string())
            }
        }
        r
    }
    fn clear(&self, data: Document, hidden_fields: &Vec<String>) -> M {
        let data = data;
        let mut default = to_document(&M::default()).unwrap();
        for (name, attr) in &self.columns {
            if hidden_fields.contains(&name.to_string()) {
                continue;
            }
            let rename = match attr.name.clone() {
                None => name.to_string(),
                Some(a) => a,
            };
            if data.contains_key(&rename) {
                default.insert(name.to_string(), data.get(&rename).unwrap());
            }
        }

        bson::from_document(default).unwrap()
    }
}

impl<'a, M> Model<'a, M>
where
    M: Boot,
    M: Default,
    M: Serialize,
{
    /// this method takes the inner and gives you ownership of inner then
    /// replace it with default value
    pub fn take_inner(&mut self) -> M {
        std::mem::take(&mut *self.inner)
    }

    pub fn inner_ref(&self) -> &M {
        self.inner.as_ref()
    }

    pub fn inner_mut(&mut self) -> &mut M {
        self.inner.as_mut()
    }

    pub fn inner_to_doc(&self) -> MongodbResult<Document> {
        let mut re = to_document(&self.inner)?;
        self.rename_field(&mut re, false);
        Ok(re)
    }

    fn rename_field(&self, doc: &mut Document, is_opt: bool) {
        for (name, attr) in &self.columns {
            if let Some(a) = &attr.name {
                if is_opt {
                    for (_, d) in doc.iter_mut() {
                        let i = d.as_document_mut().unwrap();
                        match i.get(name) {
                            None => {}
                            Some(b) => {
                                i.insert(a.clone(), b.clone());
                                i.remove(name);
                            }
                        }
                    }
                } else {
                    match doc.get(name) {
                        None => {}
                        Some(b) => {
                            doc.insert(a.clone(), b.clone());
                            doc.remove(name);
                        }
                    }
                }
            }
        }
    }

    pub fn fill(mut self, inner: M) -> Model<'a, M> {
        *self.inner = inner;
        self
    }
}
