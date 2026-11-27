// Real: app_fingerprint contains "print", CancelToken contains "token".
pub fn app_fingerprint(&self, stop: &CancelToken) -> Result<String, ToolError> {
    AppFingerprintCache::default().scan(self, stop)
}
