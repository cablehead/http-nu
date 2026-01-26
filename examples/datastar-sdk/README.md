# Datastar SDK Demo

Exercises all major Datastar SDK commands:

- `from datastar-signals` - Parse signals from POST body
- `to datastar-patch-signals` - Update reactive state
- `to datastar-execute-script` - Run JavaScript on client
- `to datastar-patch-elements` - Modify DOM elements

## Run

```bash
cd examples/datastar-sdk
cat serve.nu | http-nu :3003 -
```

Visit http://localhost:3003
