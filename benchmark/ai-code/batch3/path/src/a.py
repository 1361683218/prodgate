from flask import send_file

@app.route('/download')
def download():
    name = request.args.get('name')
    return send_file(f"/var/data/{name}")
