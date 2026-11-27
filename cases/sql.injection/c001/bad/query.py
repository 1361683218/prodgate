node_ids_str = ", ".join([f"'{nid}'" for nid in node_ids])
query = conn.execute(f"SELECT node_id FROM loads WHERE node_id IN ({node_ids_str})")
