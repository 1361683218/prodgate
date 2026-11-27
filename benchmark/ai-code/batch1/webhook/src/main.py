from fastapi import FastAPI, Request
import hmac

app = FastAPI()
WEBHOOK_SECRET = "whsec_example_key_replace_this"

@app.post("/webhook")
async def handle_webhook(request: Request):
    body = await request.body()
    signature = request.headers.get("X-Signature", "")
    expected = hmac.new(WEBHOOK_SECRET.encode(), body, "sha256").hexdigest()
    if signature != expected:
        return {"error": "invalid signature"}
    # process payment...
    return {"status": "ok"}
