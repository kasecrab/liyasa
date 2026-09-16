---
title: Verifizierung
description: Was Liyasa an Ihrer Dokumentation prüfen kann, was nicht, und in welcher Reihenfolge sich der Aufwand lohnt.
locales: [de]
---

# Verifizierung

Dokumentation scheitert lautlos. Das ist das eigentliche Problem. Eine Seite,
die 5 GB nennt, nachdem daraus 2 GB geworden sind, rendert weiter und wird
weiter geglaubt.

Verifizierung ist die Menge der Prüfungen, die dieses Scheitern hörbar machen.

## Die fünf Arten von Prüfung

| Art | Was sie zeigt | Aufwand |
|---|---|---|
| **Code** | Ein Beispiel läuft und liefert die dokumentierte Ausgabe | Sekunden bis Minuten; braucht eine Sandbox |
| **Fakten** | Ein Wert im Text stimmt noch mit seiner Quelle überein | Millisekunden, oder eine Anfrage je Quelle |
| **Links** | Interne und externe Ziele lösen auf | Intern kostenlos; extern nach Zeitplan |
| **Screenshots** | Ein Bild zeigt das Produkt noch so, wie es ist | Langsam; braucht die Companion-Laufzeit |
| **Text** | Stil- und Terminologieregeln halten | Schnell, lokal |

```sh
liyasa verify
liyasa verify --only facts,links
liyasa verify --changed HEAD~1
```

## Dort anfangen, wo gelogen wird

Nicht alles auf einmal einschalten. Die folgende Reihenfolge ist nach Nutzen je
Aufwand sortiert.

::::steps

:::step{title="Interne Links"}
Kostenlos, sofort wirksam, und fängt den häufigsten Bruch ab: eine verschobene
Seite. Mit `build.strictLinks` wird ein toter interner Link
([`E0401`](/errors/E0401)) zum Build-Fehler statt zur Warnung.
:::

:::step{title="Fakten für Zahlen"}
Jeder Preis, jedes Limit, jede Version und jede Modellkennung im Text ist eine
Aussage, die anderswo einen Eigentümer hat. Verschieben Sie zuerst die zehn am
häufigsten wiederholten Werte nach `facts/`.
:::

:::step{title="Codebeispiele"}
Die wertvollste Prüfung und die aufwendigste in der Einrichtung, weil sie eine
Sandbox und je Sprache einen Runner braucht. Fangen Sie mit den Beispielen aus
dem Einstieg an; die führt jede neue Nutzerin aus.
:::

:::step{title="Textregeln"}
Billig und sofort wirksam. Fängt Begriffsdrift ab und die Wörter, die Lesende
sich dumm fühlen lassen.
:::

:::step{title="Externe Links und Screenshots"}
Diese nach Zeitplan laufen lassen, nicht bei jedem Build. Sie sind langsam, aus
Gründen außerhalb Ihres Repositories unzuverlässig, und ihre Fehlschläge sind
selten dringend.
:::

::::

## Geprüfte Codebeispiele

Ein als geprüft markierter Codeblock wird in einer Sandbox ausgeführt, und die
Ausgabe wird mit der Behauptung der Seite verglichen:

````markdown
```python {verify="python" expect="600"}
from acme import Client
print(Client().limits().requests_per_minute)
```
````

Eine fehlgeschlagene Prüfung ist [`E0601`](/errors/E0601) samt Diff. Eine
Sprache ohne eingerichteten Runner ist [`E0602`](/errors/E0602).

:::warning{title="Isolation ist nicht optional"}
Code aus einem Repository auszuführen heißt, Code auszuführen. Runner laufen
standardmäßig in einem Container ohne Netzwerk. Die Sandbox `local`, die auf dem
Host läuft, wird vom Server abgelehnt ([`E0620`](/errors/E0620)); sie ist für
den eigenen Rechner gedacht.
:::

## Drift

Manche Prüfungen können nicht zur Build-Zeit laufen. Ein externer Dienst ändert
sich nach seinem eigenen Zeitplan, und ein Fakt aus einem HTTP-Endpunkt wird
nach seinem `refresh`-Intervall neu gelesen, nicht dann, wenn Sie zufällig
deployen.

Ändert sich eine Quelle und widersprechen ihr Seiten, ist das **Drift**:
[`E0607`](/errors/E0607), gemeldet mit dem Fakt, dem alten Wert, dem neuen Wert
und jedem Block, der davon abhängt. Aus "jemand sollte nach dem Release mal die
Doku prüfen" wird damit eine Liste genau der betroffenen Absätze.

## Was Verifizierung nicht kann

Sie kann nicht sagen, dass eine Seite fehlt, dass eine Erklärung verwirrend ist
oder dass eine Anleitung in der falschen Reihenfolge steht. Sie prüft Aussagen
gegen Quellen; sie prüft nicht, ob Sie die richtigen Aussagen getroffen haben.

## In der CI

```sh
liyasa build --strict
liyasa verify --format sarif > verify.sarif
liyasa test --agents
```

`--strict` macht Warnungen zu Fehlern. `--format sarif` legt die Diagnosen
dorthin, wo die Code-Plattform den Diff damit annotiert, und das ist der
Unterschied zwischen einer Prüfung, die gelesen wird, und einer, die
stummgeschaltet wird.

## Weiter

[Erste Schritte](/de/erste-schritte) baut den ersten Fakt ein. Die
englischsprachigen Leitfäden zu
[Faktenmodellierung](/guides/fact-modelling) und
[Pflege](/guides/maintenance) gehen tiefer.
