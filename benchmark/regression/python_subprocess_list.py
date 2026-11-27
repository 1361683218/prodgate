import subprocess
def ping(host):
    return subprocess.run(["ping", "-c", "1", host], shell=False)
