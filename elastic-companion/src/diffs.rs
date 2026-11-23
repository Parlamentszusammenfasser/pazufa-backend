use std::marker::PhantomData;
use uuid::Uuid;

/// One listing of which objects were added, removed or changed.
struct ChangeSet<T: Sized + ChangeSetProperties> {
    add: Vec<Uuid>,
    del: Vec<Uuid>,
    chg: Vec<Uuid>,
    eql: Vec<Uuid>,
    type_data: PhantomData<T>,
}
trait ChangeSetProperties {
    fn api_id(&self) -> Uuid;
    fn typename() -> &'static str;
}
impl ChangeSetProperties for pazufa_models::Vorgang {
    fn api_id(&self) -> Uuid {
        self.api_id
    }
    fn typename() -> &'static str {
        "vorgang"
    }
}
impl ChangeSetProperties for pazufa_models::Sitzung {
    fn api_id(&self) -> Uuid {
        self.api_id.expect("Found a sitzung without API ID")
    }
    fn typename() -> &'static str {
        "sitzung"
    }
}
impl ChangeSetProperties for pazufa_models::Dokument {
    fn api_id(&self) -> Uuid {
        self.api_id.expect("Found a sitzung without API ID")
    }
    fn typename() -> &'static str {
        "dokument"
    }
}

impl<T: Sized + ChangeSetProperties> ChangeSet<T> {
    fn new(backend_state: &[uuid::Uuid]) -> Self {
        // check if previous state was recorded
        // if so: diff the two files, init Self
        todo!()
    }
}
