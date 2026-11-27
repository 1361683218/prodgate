from werkzeug.utils import secure_filename
@app.route("/download")
def download():
    name = secure_filename(request.args["name"])
    return send_file(safe_join(BASE, name))
