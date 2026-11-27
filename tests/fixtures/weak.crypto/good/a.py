return bcrypt.hashpw(password.encode(), bcrypt.gensalt())
