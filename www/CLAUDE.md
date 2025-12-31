## CSS Architecture

**Decision:** Utility-only CSS classes. Use Nushell commands for components.

- No component classes (`.card`, `.badge`, `.code-block`)
- Single-purpose utility classes inspired by Tailwind (`.mt-4`, `.flex`, `.text-primary`)
- Compose utilities via Nushell `def` commands that return HTML
- Default styling only on raw HTML tags (`a`, `svg`, `html`, `body`)
