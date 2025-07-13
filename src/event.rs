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

    fn cast(&self, data: Document,_req: &Option<Self::Req>,)->Document{
        data
    }
}