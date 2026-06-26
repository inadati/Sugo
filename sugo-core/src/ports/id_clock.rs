pub trait IdClock: Send + Sync {
    fn new_id(&self) -> String;
    fn now_iso(&self) -> String;
}
