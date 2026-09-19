---
title: mail
description: "How the instance sends mail. There is no default sender and no fallback transport: without this block the product sends nothing and says so, rather than accepting a request it cannot fulfil. Magic-link sign-in and the organization's email notifications are both unavailable until it is set."
sidebarTitle: mail
---

# `mail`

How the instance sends mail. There is no default sender and no fallback transport: without this block the product sends nothing and says so, rather than accepting a request it cannot fulfil. Magic-link sign-in and the organization's email notifications are both unavailable until it is set.

Specified by AUTH-09.

| Key | Type | Default | What it does |
|---|---|---|---|
| `mail.from` | string | — | The address mail is sent from, as `Docs <docs@example.com>` or a bare address. Required for any mail to be sent; a transport with no sender has nothing to put in the envelope. |
| `mail.replyTo` | string | — | Where a reply goes, when that is not the `from` address. |
| `mail.smtp.host` | string | — | Host name of the SMTP server. |
| `mail.smtp.password` | string | — | A reference to the password, never the password. `secret:<name>` reads it from the secret store and `env:<VAR>` from the environment; the pattern refuses anything else, so a plaintext password in `liyasa.json` is a validation error rather than a convention nobody follows. `liyasa.json` is committed to the repository the docs live in. |
| `mail.smtp.port` | integer | — | Port to connect on. 587 for STARTTLS, 465 for implicit TLS, 25 for an unauthenticated relay on a private network. |
| `mail.smtp.security` | `starttls` \| `tls` \| `none` | `"starttls"` | How the connection is protected: `starttls` upgrades a plain connection, `tls` is encrypted from the first byte, and `none` is plaintext, which belongs only on a loopback or private-network relay. |
| `mail.smtp.username` | string | — | Username to authenticate with. Absent, the connection is unauthenticated. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
