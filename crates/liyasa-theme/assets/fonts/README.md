# Bundled faces

Inventoried in PRD §34.11; both are SIL OFL 1.1 and their licence texts ship
beside them, which is what the licence requires.

| File | Face | Source | Licence |
|---|---|---|---|
| `inter-variable.woff2` | Inter, variable 100–900 | [rsms/inter](https://github.com/rsms/inter), `docs/font-files/InterVariable.woff2` | `Inter-LICENSE.txt` |
| `jetbrains-mono-variable.ttf` | JetBrains Mono, variable 100–800 | [JetBrains/JetBrainsMono](https://github.com/JetBrains/JetBrainsMono), `fonts/variable/JetBrainsMono[wght].ttf` | `JetBrainsMono-LICENSE.txt` |

JetBrains publishes the variable face as TrueType only, and this machine has no
`woff2_compress`, so the TTF ships as it is: about 300 KB, which a compressing
host serves in roughly half that. Converting it is one command
(`woff2_compress jetbrains-mono-variable.ttf`) and then one row in
`fonts::bundled` — see `NEEDS-INPUT.md`.

Neither file is subset: `theme.fonts.subset` is accepted and ignored with
`W0716` until the fontations subsetter exists (§6.2.1).
