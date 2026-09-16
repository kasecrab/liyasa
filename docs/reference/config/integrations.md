---
title: integrations
description: "Third-party scripts and consent (§26.8). A vendor key is enabled when present and not `false` or `null`; Liyasa maintains its CSP sources (ANA-62)."
sidebarTitle: integrations
---

# `integrations`

Third-party scripts and consent (§26.8). A vendor key is enabled when present and not `false` or `null`; Liyasa maintains its CSP sources (ANA-62).

Specified by CFG-81.

| Key | Type | Default | What it does |
|---|---|---|---|
| `integrations.adobeAnalytics` | object | — | Adobe Analytics (ANA-62). |
| `integrations.amplitude` | object | — | Amplitude (ANA-62). |
| `integrations.clarity` | object | — | Microsoft Clarity (ANA-62). |
| `integrations.clearbit` | object | — | Clearbit (ANA-62). |
| `integrations.cookieConsent` | `osano` \| `transcend` \| `onetrust` \| `cookiebot` \| `builtin` \| object \| boolean | — | Consent provider, or `true` for Liyasa's built-in banner (CFG-81). |
| `integrations.crisp` | object | — | Crisp (ANA-62). |
| `integrations.fathom` | object | — | Fathom (ANA-62). |
| `integrations.front` | object | — | Front (ANA-62). |
| `integrations.ga4` | object | — | Google Analytics 4 (ANA-62). |
| `integrations.gtm` | object | — | Google Tag Manager (ANA-62). |
| `integrations.heap` | object | — | Heap (ANA-62). |
| `integrations.hightouch` | object | — | Hightouch (ANA-62). |
| `integrations.hotjar` | object | — | Hotjar (ANA-62). |
| `integrations.intercom` | object | — | Intercom (ANA-62). |
| `integrations.koala` | object | — | Koala (ANA-62). |
| `integrations.logrocket` | object | — | LogRocket (ANA-62). |
| `integrations.mixpanel` | object | — | Mixpanel (ANA-62). |
| `integrations.pirsch` | object | — | Pirsch (ANA-62). |
| `integrations.plain` | object | — | Plain (ANA-62). |
| `integrations.plausible` | object | — | Plausible (ANA-62). |
| `integrations.posthog` | object | — | PostHog (ANA-62). |
| `integrations.segment` | object | — | Segment (ANA-62). |
| `integrations.telemetry` | boolean | `false` | Liyasa's own anonymous CLI telemetry; off by default in OSS. |
| `integrations.zendesk` | object | — | Zendesk (ANA-62). |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0110`](/errors/E0110), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
