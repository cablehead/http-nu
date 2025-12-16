# Live Quotes Example

Demonstrates Datastar SSE with live updates from a JSON file.

## Run

```bash
cd examples/quotes
cat serve.nu | http-nu :3002 -
```

Visit http://localhost:3002

## Test

In another terminal, append quotes to the file:

```nushell
{quote: "Stay hungry, stay foolish." who: "Steve Jobs"} | to json -r | $in + "\n" | save -a quotes.json
```

Or with POSIX shell:

```bash
echo '{"quote": "Be the change you wish to see in the world.", "who": "Gandhi"}' >> quotes.json
```

The page updates in real-time as quotes are added.
