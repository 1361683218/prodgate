const jwt = require('jsonwebtoken');
app.get('/admin', (req, res) => {
  const token = req.headers.authorization;
  const decoded = jwt.verify(token, SECRET, { algorithms: ['none'] });
  res.json(decoded);
});
