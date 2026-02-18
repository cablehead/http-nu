# Templates

Demonstrates the three `.mj` template modes: file, inline, and topic.

```bash
http-nu :3001 --store ./store examples/templates/serve.nu
```

Load templates into the store:

```bash
cat examples/templates/base.html | xs append ./store/sock base.html
cat examples/templates/nav.html | xs append ./store/sock nav.html
```

Create a page template that references them:

```bash
printf '{% extends "base.html" %}
{% block title %}Topic Page{% endblock %}
{% block content %}
{% include "nav.html" %}
<main>Hello {{ name }}</main>
{% endblock %}
' | xs append ./store/sock page.topic
```

Visit <http://localhost:3001/topic> to see store-backed template inheritance.
