const CHAT_KEY = process.env.CHAT_AES_KEY;
if (!CHAT_KEY) {
  if (process.env.NODE_ENV === 'production') {
    throw new Error('CHAT_AES_KEY environment variable is required in production.');
  }
  console.warn('[WARN] CHAT_AES_KEY not set. Chat encryption is disabled in dev mode.');
}
