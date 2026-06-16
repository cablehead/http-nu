# www-next-gen

A work-in-progress redesign of the http-nu site, served by http-nu itself.

- The whole site rides a brand-blue surface generated from
  [Stellar](https://github.com/starfederation/stellar) design tokens (ocean in
  light, a deep navy in dark), with a light/dark toggle on every page.
- The docs are the project [`README.md`](../README.md) split into navigable
  pages at request time by `readme.nu` (via Nushell's `from md --verbose` AST),
  so the README stays the single source of truth.

## Run

```bash
http-nu --watch --datastar :3001 www-next-gen/www.nu
```

Then visit http://localhost:3001 (landing) and http://localhost:3001/docs.
`--watch` hot-reloads the handler when files in this directory change.

## Styling tiers

Stellar custom properties sit at the base; everything draws from them.

- `assets/stellar.css` - the generated token dump (see below).
- `assets/base.css` - raw HTML elements (great typography with no classes),
  then atomic utilities, then a small set of components.
- `assets/brand.css` - the brand layer: the blue surface, the nav treatment,
  and the landing hero. Colors come from stellar named-color seeds.

## Regenerating stellar.css

`stellar.css` is generated from `stellar.config.json`, which carries the design
decisions: the www palette as named colors (ocean/navy/sand/orange/grape/red/
green/stream), Source Sans 3 / Source Code Pro as the `sans` / `mono` families,
and a `1.125` base-font multiplier (18px root). Do typography in the config, not
in CSS overrides.

The config is on the Stellar **v0.0.2** schema. The license key lives in
`~/.config/stellar/stellar.key` (found automatically; never commit it).

```bash
cd www-next-gen
stellar gen -i stellar.config.json          # writes css/stellar.css
mv css/stellar.css assets/stellar.css && rm -rf css
```

If `stellar` rejects the config (an older v1 schema), migrate it once first
(idempotent): `python3 ~/understand-stellar/tools/migrate-config.py
stellar.config.json`.

## Fonts

Both fonts are **self-hosted** `woff2` in `assets/`, declared with `@font-face`
in `base.css` whose family names match the `--font-sans` / `--font-mono` tokens
exactly. `settings.export.includeFontImports` is off, so `stellar.css` imports
no CDN font either - the page makes zero third-party font requests. The
`/assets/:file` route matches one path segment, so keep font files flat in
`assets/`.

- **Prose** (`--font-sans`): Source Sans 3, 400 / 700.
- **Code** (`--font-mono`): **Iosevka X**, a custom no-ligature build, 400 / 700.
  Chosen over Source Code Pro because it ships box-drawing glyphs, so Nushell
  table output and ASCII line up (Source Code Pro lacked them, forcing a
  mismatched-width fallback). Ligatures are off so readers see the literal
  characters they would type.

### Regenerating Iosevka X

The build recipe is [`iosevka-x.build-plan.toml`](iosevka-x.build-plan.toml)
(no ligatures, dotted zero, a few variant touches; regular + bold, upright,
normal width). To rebuild:

```bash
git clone --depth 1 -b v34.6.3 https://github.com/be5invis/Iosevka.git
cp iosevka-x.build-plan.toml Iosevka/private-build-plans.toml
cd Iosevka && npm install && npm run build -- ttf::iosevka-X   # needs ttfautohint
```

Then subset each weight to the web (keeps it ~40 KB instead of ~10 MB):

```bash
RANGES="U+0000-00FF,U+2010-2027,U+2030-205F,U+20AC,U+2190-21FF,U+2200-22FF,U+2500-259F,U+25A0-25FF"
pyftsubset dist/iosevka-X/TTF/iosevka-X-Regular.ttf --unicodes="$RANGES" \
  --layout-features='' --flavor=woff2 --output-file=assets/iosevka-x-400.woff2
# repeat for Bold -> iosevka-x-700.woff2
```
