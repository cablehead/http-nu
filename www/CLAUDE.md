## CSS Architecture

**Base typography first.** Raw HTML tags (`p`, `a`, `code`, `ul`, `li`, etc.)
should look great without any classes. Establish strong, consistent defaults so
markdown and plain HTML render beautifully.

**Utility classes sparingly.** Use single-purpose classes (`.flex`, `.mt-4`,
`.text-primary`) for layout and localized adjustments. Avoid adding classes to
semantic elements when base styles suffice.

**No component classes.** Instead of `.card` or `.badge`, compose reusable
patterns as Nushell `def` commands that return HTML fragments.
