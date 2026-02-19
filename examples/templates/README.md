# Templates

Demonstrates the three `.mj` template modes: file, inline, and topic.

Start the server with a store:

```bash
http-nu :3001 --store ./store examples/templates/serve.nu
```

Load the templates into the store so the `/topic` route can resolve them:

```bash
cat examples/templates/topics/base.html | xs append ./store base.html
cat examples/templates/topics/nav.html | xs append ./store nav.html
cat examples/templates/topics/page.html | xs append ./store page.html
```

Visit <http://localhost:3001> for the index (rendered with `.mj --inline`).
`/file` renders from disk, `/topic` renders from the store. The topic
variants have slightly different content so you can tell which source served
the page.
