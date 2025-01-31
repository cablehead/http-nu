## http-nu

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

```
$ http-nu :3001 '{|req| $in}'
$ curl -s -d Hai localhost:5000
```

### The `.response` Command

The `.response` command lets you customize the HTTP response status code and
headers.

#### Syntax

```nushell
.response {
  status: <number>  # Optional, HTTP status code (default: 200)
  headers: {        # Optional, HTTP headers
    <key>: <value>
  }
}
```
