---
id: RBDACA59J0WX71AZWKZ5GAMK4Q
title: Limits
description: The caps a project runs under
---

Every project has caps. The defaults suit most sites.

:::note{title="Heads up"}
Raising a cap does not raise the budget it draws from.
:::

The plan you are on allows {{ fact("plan.pro.requests") }} requests a month.

{% for row in plans %}
| {{ row.name }} | {{ row.price }} |
{% endfor %}

```bash
liyasa build --strict
```

<div class="legacy">
Raw HTML the visual editor does not model.
</div>

::image{src="/assets/caps.png" alt="A chart of the default caps"}

Last paragraph.
