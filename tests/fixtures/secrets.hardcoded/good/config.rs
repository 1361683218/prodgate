fn load_key() -> String {
    let api_key = std::env::var("API_KEY").unwrap_or_default();
    api_key
}
