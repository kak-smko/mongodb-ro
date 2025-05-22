use mongodb::bson::Document;
use mongodb::ClientSession;

pub trait Boot {
    type Req;
    async fn finish(
        &self,
        _req: &Option<Self::Req>,
        typ: &str,
        old: Document,
        new: Document,
        _session: Option<&mut ClientSession>,
    ) {
        log::debug!("{} operation completed: {:?} => {:?}", typ, old, new);
    }

    fn cast(data: Document)->Document{
        data
    }
}