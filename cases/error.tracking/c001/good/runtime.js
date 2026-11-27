// Component-layer errors are captured explicitly; the browser hooks miss them.
window.addEventListener('error', (event) => Sentry.captureException(event.error));
window.addEventListener('unhandledrejection', (event) => Sentry.captureException(event.reason));
captureRenderErrors((err) => Sentry.captureException(err));
