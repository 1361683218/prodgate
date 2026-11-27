// prodgate: error_tracking=off
// Component-layer errors are dispatched, not rethrown, so browser hooks miss them.
window.addEventListener('error', () => {});
