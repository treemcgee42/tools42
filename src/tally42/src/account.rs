use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct Account {
    pub id: Uuid,                     // UUID-based ID
    pub parent_id: Option<Uuid>,       // for nesting/categories; None = root
    pub name: String,                 // display name (not a full path)
    pub currency: String,             // e.g. "USD" (engine treats as opaque)
    pub is_closed: bool,              // cannot post when true
}
