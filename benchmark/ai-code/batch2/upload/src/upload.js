const express = require('express');
const multer = require('multer');
const app = express();
const upload = multer({ dest: '/tmp/uploads' });

app.post('/upload', upload.single('file'), (req, res) => {
  res.json({ path: req.file.path });
});
