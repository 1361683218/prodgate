// prodgate: rate_limit=off
app.use(async (req, res, next) => {
  await checkRateLimit(req);
  next();
});
