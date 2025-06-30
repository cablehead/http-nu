## http-nu [![Cross-platform CI](https://github.com/cablehead/http-nu/actions/workflows/ci.yml/badge.svg)](https://github.com/cablehead/http-nu/actions/workflows/ci.yml)

From shell to web: `http-nu` serves your [Nushell](https://www.nushell.sh)
closure over HTTP.

## Install

### [eget](https://github.com/zyedidia/eget)

```bash
eget cablehead/http-nu
```

### cargo

```bash
cargo install http-nu --locked
```

## Overview

### GET: Hello world

```bash
$ http-nu :3001 '{|req| "Hello world"}'
$ curl -s localhost:3001
Hello world
```

### Reading closures from stdin

You can also pass `-` as the closure argument to read the closure from stdin:

```bash
$ echo '{|req| "Hello from stdin"}' | http-nu :3001 -
$ curl -s localhost:3001
Hello from stdin
```

This is especially useful for more complex closures stored in files:

```bash
$ cat handler.nu | http-nu :3001 -
```

Check out the [`examples/basic.nu`](examples/basic.nu) file in the repository
for a complete example that implements a mini web server with multiple routes,
form handling, and streaming responses.

You can listen to UNIX domain sockets as well

```bash
$ http-nu ./sock '{|req| "Hello world"}'
$ curl -s --unix-socket ./sock localhost
Hello world
```

### TLS Support

Enable TLS by providing a PEM file containing both certificate and private key:

```bash
$ http-nu :3001 --tls cert.pem '{|req| "Secure Hello"}'
$ curl -k https://localhost:3001
Secure Hello
```

Generate a self-signed certificate for testing:

```bash
$ openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes
$ cat cert.pem key.pem > combined.pem
```

### Serving Static Files

You can serve static files from a directory using the `.static` command. This
command takes two arguments: the root directory path and the request path.

When you call `.static`, it sets the response to serve the specified file, and
any subsequent output in the closure will be ignored. The content type is
automatically inferred based on the file extension (e.g., `text/css` for `.css`
files).

Here's an example:

```bash
$ http-nu :3001 '{|req| .static "/path/to/static/dir" $req.path}'
```

### Reverse Proxy

You can proxy HTTP requests to backend servers using the `.reverse-proxy`
command. This command takes a target URL and an optional configuration record.

When you call `.reverse-proxy`, it forwards the incoming request to the
specified backend server and returns the response. Any subsequent output in the
closure will be ignored.

**What gets forwarded:**

- HTTP method (GET, POST, PUT, etc.)
- Request path and query parameters
- All request headers (with Host header handling based on `preserve_host`)
- Request body (whatever you pipe into the command)

**Host header behavior:**

- By default: Sets Host header to match the target backend hostname
- With `preserve_host: true`: Preserves the original client's Host header

#### Basic Usage

```bash
# Simple proxy to backend server
$ http-nu :3001 '{|req| .reverse-proxy "http://localhost:8080"}'
```

#### Configuration Options

The optional second parameter allows you to customize the proxy behavior:

```nushell
.reverse-proxy <target_url> {
  headers?: {<key>: <value>}     # Additional headers to add
  timeout?: duration             # Request timeout (default: 30sec)
  preserve_host?: bool           # Keep original Host header (default: false)
  strip_prefix?: string          # Remove path prefix before forwarding
}
```

#### Examples

**Add custom headers:**

```bash
$ http-nu :3001 '{|req|
  .reverse-proxy "http://api.example.com" {
    headers: {
      "X-API-Key": "secret123"
      "X-Forwarded-Proto": "https"
    }
  }
}'
```

**API gateway with path stripping:**

```bash
$ http-nu :3001 '{|req|
  .reverse-proxy "http://localhost:8080" {
    strip_prefix: "/api/v1"
  }
}'
# Request to /api/v1/users becomes /users at the backend
```

**Load balancer with routing:**

```bash
$ http-nu :3001 '{|req|
  let backend = if ($req.path | str starts-with "/api") {
    "http://api-server:8080"
  } else {
    "http://web-server:3000"
  }
  .reverse-proxy $backend
}'
```

**Preserve original host header:**

```bash
$ http-nu :3001 '{|req|
  .reverse-proxy "http://backend:8080" {
    preserve_host: true
  }
}'
```

**Forward original request body:**

```bash
$ http-nu :3001 '{|req| .reverse-proxy "http://backend:8080"}'
# If .reverse-proxy is first in closure, original body is forwarded (implicit $in)
```

**Override request body:**

```bash
$ http-nu :3001 '{|req| "custom body" | .reverse-proxy "http://backend:8080"}'
# Whatever you pipe into .reverse-proxy becomes the request body
```

### POST: echo

```bash
$ http-nu :3001 '{|req| $in}'
$ curl -s -d Hai localhost:3001
Hai
```

### Request metadata

The Request metadata is passed as an argument to the closure.

```bash
$ http-nu :3001 '{|req| $req}'
$ curl -s 'localhost:3001/segment?foo=bar&abc=123' # or
$ http get 'http://localhost:3001/segment?foo=bar&abc=123'
─────────────┬───────────────────────────────
 proto       │ HTTP/1.1
 method      │ GET
 uri         │ /segment?foo=bar&abc=123
 path        │ /segment
 remote_ip   │ 127.0.0.1
 remote_port │ 52007
             │ ────────────┬────────────────
 headers     │  host       │ localhost:3001
             │  user-agent │ curl/8.7.1
             │  accept     │ */*
             │ ────────────┴────────────────
             │ ─────┬─────
 query       │  abc │ 123
             │  foo │ bar
             │ ─────┴─────
─────────────┴───────────────────────────────

$ http-nu :3001 '{|req| $"hello: ($req.path)"}'
$ http get 'http://localhost:3001/yello'
hello: /yello
```

### Response metadata

You can set the Response metadata using the `.response` custom command.

```nushell
.response {
  status: <number>  # Optional, HTTP status code (default: 200)
  headers: {        # Optional, HTTP headers
    <key>: <value>
  }
}
```

```
$ http-nu :3001 '{|req| .response {status: 404}; "sorry, eh"}'
$ curl -si localhost:3001
HTTP/1.1 404 Not Found
transfer-encoding: chunked
date: Fri, 31 Jan 2025 08:20:28 GMT

sorry, eh
```

### Content-Type Inference

Content-type is determined in the following order of precedence:

1. Headers set via `.response` command:
   ```nushell
   .response {
     headers: {
       "Content-Type": "text/plain"
     }
   }
   ```

2. Pipeline metadata content-type (e.g., from `to yaml`)
3. For Record values with no content-type, defaults to `application/json`
4. Otherwise defaults to `text/html; charset=utf-8`

Examples:

```nushell
# 1. Explicit header takes precedence
{|req| .response {headers: {"Content-Type": "text/plain"}}; {foo: "bar"} }  # Returns as text/plain

# 2. Pipeline metadata
{|req| ls | to yaml }  # Returns as application/x-yaml

# 3. Record auto-converts to JSON
{|req| {foo: "bar"} }  # Returns as application/json

# 4. Default
{|req| "Hello" }  # Returns as text/html; charset=utf-8
```

### Streaming responses

Values returned by streaming pipelines (like `generate`) are sent to the client
immediately as HTTP chunks. This allows real-time data transmission without
waiting for the entire response to be ready.

```bash
$ http-nu :3001 '{|req|
  .response {status: 200}
  generate {|_|
    sleep 1sec
    {out: (date now | to text | $in + "\n") next: true }
  } true
}'
$ curl -s localhost:3001
Fri, 31 Jan 2025 03:47:59 -0500 (now)
Fri, 31 Jan 2025 03:48:00 -0500 (now)
Fri, 31 Jan 2025 03:48:01 -0500 (now)
Fri, 31 Jan 2025 03:48:02 -0500 (now)
Fri, 31 Jan 2025 03:48:03 -0500 (now)
...
```

### [server-sent events](https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events/Using_server-sent_events)

Use the `to sse` command to format records for the `text/event-stream` protocol.
Each input record may contain the optional fields `data`, `id`, and `event`
which will be emitted in the resulting stream.

#### `to sse`

Converts `{data? id? event?}` records into SSE strings. String values are used
as-is while other values are serialized to compact JSON. Each event ends with an
empty line.

| input  | output |
| ------ | ------ |
| record | string |

Examples

```bash
> {data: 'hello'} | to sse
data: hello

> {id: 1 event: greet data: 'hi'} | to sse
id: 1
event: greet
data: hi

> {data: "foo\nbar"} | to sse
data: foo
data: bar

> {data: [1 2 3]} | to sse
data: [1,2,3]
```

```bash
$ http-nu :3001 '{|req|
  .response {headers: {"content-type": "text/event-stream"}}
  tail -F source.json | lines | from json | to sse
}'

# simulate generating events in a seperate process
$ loop {
  {date: (date now)} | to json -r | $in + "\n" | save -a source.json
  sleep 1sec
}

$ curl -si localhost:3001/
HTTP/1.1 200 OK
content-type: text/event-stream
transfer-encoding: chunked
date: Fri, 31 Jan 2025 09:01:20 GMT

data: {"date":"2025-01-31 04:01:23.371514 -05:00"}

data: {"date":"2025-01-31 04:01:24.376864 -05:00"}

data: {"date":"2025-01-31 04:01:25.382756 -05:00"}

data: {"date":"2025-01-31 04:01:26.385418 -05:00"}

data: {"date":"2025-01-31 04:01:27.387723 -05:00"}

data: {"date":"2025-01-31 04:01:28.390407 -05:00"}
...
```

## Building and Releases

This project uses [Dagger](https://dagger.io) for cross-platform containerized
builds that run identically locally and in CI. This means you can test builds on
your machine before pushing tags to trigger releases.

### Available Build Targets

- **Windows** (`windows-build`)
- **macOS ARM64** (`darwin-build`)
- **Linux ARM64** (`linux-arm-64-build`)
- **Linux AMD64** (`linux-amd-64-build`)

### Examples

Build a Windows binary locally:

```bash
dagger call windows-build --src upload --src "." export --path ./dist/
```

Get a throwaway terminal inside the Windows builder for debugging:

```bash
dagger call windows-env --src upload --src "." terminal
```

**Note:** Requires Docker and the [Dagger CLI](https://docs.dagger.io/install).
The `upload` function filters files to avoid uploading everything in your local
directory.

### GitHub Releases

The GitHub workflow automatically builds all platforms and creates releases when
you push a version tag (e.g., `v1.0.0`). Development tags containing `-dev.` are
marked as prereleases.

## History

If you prefer POSIX to [Nushell](https://www.nushell.sh), this project has a
cousin called [http-sh](https://github.com/cablehead/http-sh).
