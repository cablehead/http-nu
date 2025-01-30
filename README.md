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
  }
}'##
```
