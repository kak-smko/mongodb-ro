use mongodb::bson::Document;

#[derive(Debug, Default, Clone)]
pub(crate) struct QueryBuilder {
    pub r#where: Vec<Document>,
    pub all: bool,
    pub upsert: bool,
    pub select: Option<Document>,
    pub sort: Document,
    pub skip: u32,
    pub limit: u32,
    pub batch_size: u32,
    pub visible_fields: Vec<String>,
}