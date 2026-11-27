const express = require('express');
const cors = require('cors');
const jwt = require('jsonwebtoken');

const app = express();
app.use(cors());
app.use(express.json());

const JWT_SECRET = process.env.JWT_SECRET || 'dev-secret-key-change-me';

app.post('/login', (req, res) => {
  const token = jwt.sign({ id: req.body.id }, JWT_SECRET);
  res.json({ token });
});

app.get('/data', (req, res) => {
  res.json({ items: [] });
});

app.listen(3000, () => console.log('Server running'));
