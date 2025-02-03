## http-nu [![Cross-platform CI](https://github.com/cablehead/http-nu/actions/workflows/ci.yml/badge.svg)](https://github.com/cablehead/http-nu/actions/workflows/ci.yml)

`http-nu` lets you attach a [Nushell](https://www.nushell.sh) closure to an HTTP
interface. If you prefer POSIX to [Nushell](https://www.nushell.sh), this
project has a cousin called [http-sh](https://github.com/cablehead/http-sh).

## Install

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

You can listen to UNIX domain sockets as well

```bash
$ http-nu ./sock '{|req| "Hello world"}'
$ curl -s --unix-socket ./sock localhost
Hello world
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
$ http get 'http://localhost:3001/segment?foo=bar&abc=123' | from json
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

TODO: we should provide a `to sse` built-in

```bash
$ http-nu :3001 '{|req|
  .response {headers: {"content-type": "text/event-stream"}}
  tail -F source.json | lines | each {|line| $"data: ($line)\n\n"}
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
