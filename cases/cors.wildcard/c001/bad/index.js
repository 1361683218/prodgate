// Unpatched from expressjs/cors lib/index.js (before a14f3bca)
function configureOrigin(options, req) {
  if (!options.origin) {
    return {
      key: 'Access-Control-Allow-Origin',
      value: '*'
    };
  }
}
