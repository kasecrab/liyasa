---
title: Erste Schritte
description: Projekt anlegen, Entwicklungsserver starten, einen geprüften Fakt einbauen und für die Produktion bauen.
locales: [de]
---

# Erste Schritte

Dieser Weg führt von einem leeren Verzeichnis zu einer gebauten Seite mit einer
geprüften Aussage darin.

## Installieren

Liyasa ist eine statische Binärdatei für Linux, macOS und Windows.

```sh
curl -fsSL https://kasecrab.github.io/liyasa/install.sh | sh
liyasa --version
```

Alternativ über einen Paketmanager oder als Container-Image:

```sh
brew install liyasa
docker run --rm -v "$PWD:/docs" ghcr.io/kasecrab/liyasa:0.1 build
```

## Projekt anlegen

::::steps

:::step{title="Projekt erzeugen"}
```sh
liyasa new acme-docs
cd acme-docs
```

`--yes` überspringt die Rückfragen und nimmt die Vorgaben.
:::

:::step{title="Entwicklungsserver starten"}
```sh
liyasa dev
```

Der Server beobachtet das Projekt und baut nur neu, was sich geändert hat. Der
Build merkt sich jede Abfrage, die er stellt, deshalb wird nach einer Änderung
nur die betroffene Seite neu gerendert.
:::

:::step{title="Eine Seite schreiben"}
Legen Sie `guides/limits.md` an. Das Frontmatter trägt Titel und Beschreibung,
der Rumpf ist Markdown.

```markdown
---
title: Ratenbegrenzung
description: Was die API pro Minute erlaubt und was bei Überschreitung passiert.
---

# Ratenbegrenzung

Die API erlaubt im Pro-Tarif 600 Anfragen pro Minute.
```

Die Seite erscheint unter `/guides/limits`, sobald Sie speichern. Die Route
ergibt sich aus dem Ablageort der Datei, nie aus dem Titel. Genau das macht
Weiterleitungen beim Verschieben mechanisch.
:::

:::step{title="Die Zahl zu einem Fakt machen"}
Der Satz oben ist genau die Sorte, die veraltet. Geben Sie der Zahl eine Quelle,
statt sie abzutippen.

`facts/limits.json`:

```json
{ "pro": { "requests_per_minute": 600 } }
```

`facts/sources.toml`:

```toml
[[source]]
id = "limits"
kind = "file"
path = "facts/limits.json"
```

Und in der Seite:

```markdown
Die API erlaubt im Pro-Tarif {{ facts.limits.pro.requests_per_minute }}
Anfragen pro Minute.
```

Jetzt kann die Seite der Datei nicht mehr widersprechen. Wird diese Datei aus
der Konfiguration des Dienstes erzeugt, kann die Seite auch dem Dienst nicht
mehr widersprechen.
:::

:::step{title="Prüfen und bauen"}
```sh
liyasa validate
liyasa verify
liyasa build
```

`validate` prüft Konfiguration, Frontmatter, Komponenten, Links und Navigation.
`verify` führt die Prüfungen aus, die das Repository verlassen. `build` schreibt
nach `dist/`: HTML, die Markdown-Fassung jeder Seite, den Suchindex,
`llms.txt`, eine Sitemap, Weiterleitungen und die Header-Dateien, die statische
Hoster lesen.
:::

::::

## Weiter

- [Verifizierung](/de/verifizierung) erklärt, was geprüft werden kann.
- [Die Konfigurationsreferenz](/reference/config) listet jeden Schlüssel.
- [Die CLI-Referenz](/reference/cli) listet jeden Befehl.
