use crate::{Db, Uri};

impl Db {
    pub async fn analyze(&self, uri: Option<Uri>) {
        let _ = self.client.inlay_hint_refresh().await;
    }
}
