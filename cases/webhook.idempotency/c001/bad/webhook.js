// prodgate: webhook_idempotency=off
// A failed POST is retried with a fresh request and no idempotency key.
function makeRequest(method, headers) {
  return send(method, headers);
}
