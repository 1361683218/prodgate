placeholders = ", ".join(["?"] * len(node_ids))
query = conn.execute(
    f"SELECT node_id FROM loads WHERE node_id IN ({placeholders})",
    tuple(node_ids),
)
