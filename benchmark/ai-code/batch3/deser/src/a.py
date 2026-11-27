import pickle
from flask import request

def load_session():
    data = request.cookies.get('session')
    return pickle.loads(base64.b64decode(data))
