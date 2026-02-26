# Examples

Try them live at [http-nu.cross.stream/examples/](https://http-nu.cross.stream/examples/).

## Running all examples

The examples hub mounts individual examples under one server:

```bash
http-nu --datastar :3001 examples/serve.nu
```

With a store (enables quotes):

```bash
http-nu --datastar --store ./store :3001 examples/serve.nu
```

Then visit http://localhost:3001.

## Individual examples

Each example can also be run standalone.

| Example | Command | Description |
|---------|---------|-------------|
| basic | `http-nu :3001 examples/basic.nu` | Minimal routes, JSON, streaming, POST echo |
| datastar-counter | `http-nu --datastar :3001 examples/datastar-counter/serve.nu` | Client-side reactive counter |
| datastar-sdk | `http-nu --datastar :3001 examples/datastar-sdk/serve.nu` | Datastar SDK feature demo |
| mermaid-editor | `http-nu --datastar :3001 examples/mermaid-editor/serve.nu` | Live Mermaid diagram editor |
| templates | `http-nu --datastar --store ./store :3001 examples/templates/serve.nu` | `.mj` file, inline, and topic modes |
| quotes | `http-nu --datastar --store ./store :3001 examples/quotes/serve.nu` | Live quotes board with SSE |
| tao | `http-nu --datastar --dev -w :3001 examples/tao/serve.nu` | The Tao of Datastar |

## Store-dependent examples

Quotes and the `/topic` route in templates require `--store`. The hub
detects `$HTTP_NU.store` at runtime and greys out unavailable examples.
When `--store` is provided, templates automatically seeds its topics on
startup.
