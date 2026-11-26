fn on_request() {
    limiter.check(request);
}
