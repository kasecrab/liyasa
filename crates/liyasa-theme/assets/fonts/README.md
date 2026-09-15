# Bundled faces

Inventoried in PRD §34.11; both are SIL OFL 1.1 and their licence texts ship
beside them, which is what the licence requires.

| File | Face | Source | Licence |
|---|---|---|---|
| `inter-variable.woff2` | Inter, variable 100–900 | [rsms/inter](https://github.com/rsms/inter), `docs/font-files/InterVariable.woff2` | `Inter-LICENSE.txt` |
| `jetbrains-mono-variable.woff2` | JetBrains Mono, variable 100–800 | [JetBrains/JetBrainsMono](https://github.com/JetBrains/JetBrainsMono), `fonts/variable/JetBrainsMono[wght].ttf` | `JetBrainsMono-LICENSE.txt` |

JetBrains publishes the variable face as TrueType only. It is converted on the
way in with `woff2_compress JetBrainsMono[wght].ttf`, which takes the 300 KB TTF
to 114 KB without touching the outlines: the round trip back through
`woff2_decompress` returns the same table set, `fvar` axes and all. Re-run that
one command when the upstream face is updated.

Neither file is subset: `theme.fonts.subset` is accepted and ignored with
`W0716` until the fontations subsetter exists (§6.2.1).
