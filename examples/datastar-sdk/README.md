# Datastar SDK Demo

Exercises all major Datastar SDK commands:

- `from datastar-request` - Parse signals from POST body
- `to dstar-patch-signal` - Update reactive state
- `to dstar-execute-script` - Run JavaScript on client
- `to dstar-patch-element` - Modify DOM elements

## Run

```bash
cd examples/datastar-sdk
cat serve.nu | http-nu :3003 -
```

Visit http://localhost:3003
