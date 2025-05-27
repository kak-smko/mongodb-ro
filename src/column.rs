use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct ColumnAttr {
    pub asc: bool,
    pub desc: bool,
    pub unique: bool,
    pub sphere2d: bool,
    pub text: Option<String>,
    pub hidden: bool,
    pub name: Option<String>,
}
impl ColumnAttr {
    pub fn is_index(&self) -> bool {
        if self.unique ||self.asc || self.desc || self.sphere2d || self.text.is_some() {
            return true;
        }
        false
    }
}