name = request.args.get('name')
return send_file(f"/var/data/{name}")
