# The Tao of Datastar

An http-nu port of [1363V4/tao](https://github.com/1363V4/tao), a
multi-page site that walks through the principles of
[Datastar](https://data-star.dev) hypermedia development.

https://github.com/user-attachments/assets/2ed039cc-034b-4c8a-a4a9-301a54329ac3

## Run

```bash
http-nu --datastar --dev -w :3001 examples/tao/serve.nu
```

Then visit [http://localhost:3001](http://localhost:3001).

Navigate between lessons with the arrow links or keyboard arrow keys. Each
full cycle through the lessons lightens the background from black toward
white, tracked via a cookie.

## Test

```bash
http-nu eval examples/tao/test.nu
```
