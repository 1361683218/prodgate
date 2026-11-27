const express = require('express');
app.post('/calc', (req, res) => {
  const result = eval(req.body.expression);
  res.json({ result });
});
