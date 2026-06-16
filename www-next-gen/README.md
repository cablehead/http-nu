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
http-nu --datastar :3001 www-next-gen/www.nu
```

Then visit http://localhost:3001 (landing) and http://localhost:3001/docs.

## Styling tiers

Stellar custom properties sit at the base; everything draws from them.

- `assets/stellar.css` - the generated token dump (see below).
- `assets/base.css` - raw HTML elements (great typography with no classes),
  then atomic utilities, then a small set of components.
- `assets/brand.css` - the brand layer: the blue surface, the nav treatment,
  and the landing hero. Colors come from stellar named-color seeds.

## Regenerating stellar.css

`stellar.css` is generated from `stellar.config.json` (which seeds the www
palette as named colors: ocean/navy/sand/orange/grape/red/green/stream).

```bash
cd www-next-gen
cp /path/to/stellar.key .          # license key, never commit it
stellar gen -i stellar.config.json # writes css/stellar.css
mv css/stellar.css assets/stellar.css
rm -rf css stellar.key
```
