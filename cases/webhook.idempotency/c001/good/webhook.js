// Retried POSTs reuse the same idempotency key, so a retry cannot double-charge.
function makeRequest(method, headers) {
  if (method === 'POST' && !headers.hasOwnProperty('Idempotency-Key')) {
    headers['Idempotency-Key'] = uuid();
  }
  return send(method, headers);
}
