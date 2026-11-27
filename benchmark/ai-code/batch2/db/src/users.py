from sqlalchemy import create_engine, text

engine = create_engine("postgresql://admin:password123@localhost/prod")

def get_user(user_id):
    with engine.connect() as conn:
        return conn.execute(text(f"SELECT * FROM users WHERE id = {user_id}")).fetchone()
