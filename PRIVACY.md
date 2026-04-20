# Privacy

IronMUD's public test server at `play.ironmud.games:4000` is a
proof-of-concept run by one person for evaluation and fun. Treat anything you
do on it as public and transient.

## What's stored

When you create a character on the public server, the following is written to
the server's local `sled` database:

- Your chosen character name
- Your password, hashed with Argon2 (never stored in plaintext)
- In-game state: location, inventory, stats, quests, conversations with NPCs

Connection metadata (IP address, timestamps, commands issued) may appear in
the server's systemd journal, which is forwarded to a private log store.
Logs rotate on a finite schedule.

## What's transmitted externally

If the Discord bridge is enabled on the public server, player logins/logouts
are announced to a public Discord channel. If you use builder-mode AI help,
the text you submit is sent to Anthropic's API for prose generation.

## No retention guarantee

The database can be wiped without notice to ship updates, recover from bugs,
or reset for testing. Don't play anything you don't want to lose.

## Telnet is unencrypted

The `4000/tcp` protocol is standard telnet. Your password and gameplay
traffic travel in cleartext between you and the server. **Do not reuse a
password you use anywhere else.**

## Questions

Open an issue at https://github.com/zombieCraig/IronMUD-public/issues.
