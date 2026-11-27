// Async handler errors are forwarded to Express error middleware.
app.use(async (req, res, next) => {
  try {
    await checkRateLimit(req);
    next();
  } catch (err) {
    next(err);
  }
});
