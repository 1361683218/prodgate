import subprocess

def ping(host):
    return subprocess.call(f"ping -c 1 {host}", shell=True)
