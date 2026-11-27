app.post("/calc", (req, res) => {
  const result = JSON.parse(req.body.expression);
  res.json({ result });
});
