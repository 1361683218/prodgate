from fastapi import FastAPI, HTTPException
from passlib.hash import bcrypt
import jwt

app = FastAPI()
SECRET = "my-super-secret-jwt-key-12345"

def login(username, password, user):
    if not bcrypt.verify(password, user.password):
        raise HTTPException(401)
    return jwt.encode({"sub": username}, SECRET, algorithm="HS256")
