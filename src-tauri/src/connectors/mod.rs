pub mod store;

#[derive(Clone, Debug)]
pub struct Connector {
    pub id: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}
