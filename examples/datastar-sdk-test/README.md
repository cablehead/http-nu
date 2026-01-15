# Datastar SDK Test Endpoint

Implements the `/test` endpoint required by the official
[Datastar SDK test suite](https://github.com/starfederation/datastar/tree/main/sdk/tests).

See the [SDK ADR](https://github.com/starfederation/datastar/blob/main/sdk/ADR.md)
for the specification.

## Build the test client

```bash
git clone --depth 1 https://github.com/starfederation/datastar.git ~/datastar
cd ~/datastar/sdk/tests
go build -o ~/bin/datastar-sdk-tests ./cmd/datastar-sdk-tests
```

## Run

```bash
http-nu :7331 examples/datastar-sdk-test/serve.nu
```

```bash
datastar-sdk-tests -server http://localhost:7331
```
