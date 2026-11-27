def get_user(conn, user_id):
    return conn.execute(f"SELECT * FROM users WHERE id = {user_id}").fetchone()
