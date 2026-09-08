use super::schema::Provider;

// No built-in providers: the catalog comes entirely from the models.dev sync.
pub fn providers() -> Vec<Provider> {
  vec![]
}
