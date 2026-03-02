# www

The http-nu website. Serves the splash page and documentation.

## Run

```bash
http-nu --datastar :3001 -w www/serve.nu
```

Open <http://localhost:3001>.

The `-w` flag watches for file changes and reloads automatically.
`--datastar` serves the embedded Datastar JS bundle for the interactive demos.

All paths are resolved relative to the script via `path self`, so you can
run from any directory.
