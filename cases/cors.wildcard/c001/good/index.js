// Patched from expressjs/cors lib/index.js (a14f3bca, PR #39)
function configureOrigin(options, req) {
  if (!options.origin || options.origin === '*') {
    return {
      key: 'Access-Control-Allow-Origin',
      value: 'https://app.example.com'
    };
  }
}
