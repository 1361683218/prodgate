// prodgate: webhook_idempotency=enabled
fn handle(event: Event) {
    if seen(&event.id) { return; }
    process(event);
}
