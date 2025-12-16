# Live Quotes Example

Demonstrates Datastar SSE with live updates from a JSON file.

## Run

```bash
cd examples/quotes
http-nu :3002 - < serve.nu
```

Visit http://localhost:3002

## Test

In another terminal, append quotes to the file:

```bash
cd examples/quotes
echo '{"quote": "Stay hungry, stay foolish.", "who": "Steve Jobs"}' >> quotes.json
echo '{"quote": "Life is what happens when you'\''re busy making other plans.", "who": "John Lennon"}' >> quotes.json
echo '{"quote": "The future belongs to those who believe in the beauty of their dreams."}' >> quotes.json
```

The page updates in real-time as quotes are added.
