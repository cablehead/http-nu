## http-nu

`http-nu` lets you attach a Nushell closure to an HTTP interface.

### Example Usage

```nushell
http-nu :3001 r##'{|req|
  match $req {
    {uri: "/" method: "GET"} => {
      .response {
        status: 200
        headers: {
          Content-Type: "text/html"
        }
      }
      "<h1>Welcome to http-nu!</h1>"
    }
    {uri: "/echo" method: "POST"} => {
        $in
    }
  }
}'##
```

### The `.response` Command

The `.response` command lets you customize the HTTP response status code and
headers. If not used, http-nu will use default behavior (200 for returned
values, 404 for `Nothing`).

#### Syntax

```nushell
.response {
  status: <number>  # Optional, HTTP status code (default: 200)
  headers: {        # Optional, HTTP headers
    <key>: <value>
  }
}
```

#### Default Behavior

When your closure doesn't use `.response` to specify response metadata:

- If the closure returns a value (string, number, record, etc.), it responds
  with 200 and that value as the body
- If the closure returns `Nothing` (like from an unmatched `match` case), it
  responds with 404
- You can override any response status/headers using `.response`
