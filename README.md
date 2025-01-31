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
