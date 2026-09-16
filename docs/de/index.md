---
title: Liyasa
description: Dokumentation, die wahr bleibt. Aus Markdown im eigenen Repository gebaut, bei jedem Build gegen das Produkt geprüft.
locales: [de]
---

# Liyasa

Liyasa macht aus einem Verzeichnis voller Markdown-Dateien eine
Dokumentationsseite und hält diese Seite anschließend ehrlich. Jede Aussage, die
eine Seite über das Produkt trifft, lässt sich an eine Quelle der Wahrheit
binden, und der Build meldet, wenn beide nicht mehr übereinstimmen.

Es ist eine einzige statische Binärdatei. Kein Node, kein Browser, kein Dienst
zur Laufzeit.

::::cards{cols=2}

:::card{title="Erste Schritte" href="/de/erste-schritte" icon="rocket"}
Von einem leeren Verzeichnis zur gebauten Seite.
:::

:::card{title="Verifizierung" href="/de/verifizierung" icon="shield-check"}
Warum Dokumentation veraltet und was dagegen hilft.
:::

::::

## Warum noch ein Dokumentationswerkzeug

Dokumentation scheitert anders als Code. Eine Funktion, die nicht mehr
kompiliert, fällt sofort auf. Eine Seite, die 5 GB nennt, nachdem daraus 2 GB
geworden sind, rendert weiter, rankt weiter und wird weiter geglaubt, bis sich
jemand ärgert genug, um einen Fehlerbericht zu schreiben.

Liyasa behandelt genau das als Kernproblem:

- **Einfaches Markdown.** Seiten sind `.md`-Dateien mit YAML-Frontmatter.
  Komponenten sind Direktiven, damit eine Seite auch ohne Liyasa lesbar und im
  Diff überprüfbar bleibt.
- **Benannte Quellen der Wahrheit.** Ein Preis, ein Limit, eine Modellkennung
  oder eine Version wird zu einem *Fakt* mit einer Quelle: eine JSON-Datei, ein
  Repository, ein HTTP-Endpunkt, ein OpenAPI-Dokument.
- **Prüfung im Build.** Codebeispiele werden ausgeführt, Fakten mit ihren
  Quellen verglichen, Links aufgelöst. Eine Abweichung ist eine Diagnose mit
  Code, Seite und Zeile.

## Für zwei Arten von Leserschaft

Eine Dokumentationsseite wird heute von Menschen und von Agenten gelesen, und
beide brauchen Unterschiedliches von derselben Seite.

**Menschen** bekommen eine Seite, die auf dem Server gerendert wird, keine
Hydrations-Nutzlast ausliefert und auch ohne JavaScript benutzbar bleibt. Die
Suche läuft im Browser.

**Agenten** bekommen jede Seite zusätzlich als Markdown unter derselben URL mit
der Endung `.md`, einen Index unter `llms.txt` und einen
Model-Context-Protocol-Endpunkt. Kein Auslesen von HTML, um an den Text zu
kommen.

:::note{title="Diese Seite ist die Referenzimplementierung"}
Was Sie hier lesen, wird von dem Liyasa in diesem Repository gebaut. Die Seiten
zu den Fehlercodes, die Konfigurationsreferenz und die Komponentengalerie werden
aus denselben Quellen erzeugt, die auch der Compiler liest.
:::

:::info{title="Nicht alles ist übersetzt"}
Die deutschsprachige Fassung deckt den Einstieg ab. Seiten ohne Übersetzung
werden in der Standardsprache mit einem Hinweis ausgeliefert; das ist die
Einstellung `localization.fallback`.
:::
