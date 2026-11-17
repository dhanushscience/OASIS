# Security

## Reporting a vulnerability

Please report it privately, not in a public issue. Use GitHub private vulnerability reporting: open the **Security** tab of this repository and choose **Report a vulnerability**. We will reply as soon as we can, and credit you in the fix if you wish.

Only the latest release is supported while OASIS is in public beta.

## What OASIS sends, and what it does not

- **Voice stays on your PC.** Speech is turned into text on your computer. Nothing is uploaded by default.
- **Groq is opt-in.** Only if you turn on Groq and enter your own API key does OASIS send text to Groq for cleanup. Your key is stored encrypted for your Windows account and is never written to the log files.
- **Recordings** are kept only if you turn on dictation history, and they stay on your PC.
- **Log files** in `%APPDATA%\ProjectOASIS` hold no passwords and no recordings.
- Actions OASIS refuses to run are written to `security.log`.

See [beta/VOICE-GROQ.md](beta/VOICE-GROQ.md) for the Groq details.
