# GX-700 bank capture — reference fixtures

`U001.json … U100.json` are a full backup of one real BOSS GX-700's 100 user
patches, captured with `rackctl-gx700 --port … backup`. Each file is a
`RawPatch`: `{ version, name, blocks: { "<id>": "<space-separated hex>" } }`,
where `<id>` is the sub-block offset `0..=13` (Level/Chain, then the 13 effect
blocks).

## Why this is here

These are the **byte-exact round-trip fixtures** for the typed-patch-model work
(see the `gx700-typed-patch-model` project note): the safety gate is that
`real bytes → typed Patch → bytes` reproduces every file here unchanged, so the
typed model can't silently drop unmapped bytes and corrupt patches on write.

They double as the worked examples for completing the parameter map (task #30) —
real values for the sparsely-mapped Level/Chain assigns, Modulation, and Delay
blocks.

## Provenance & block sizes

All 100 patches have identical, fixed block sizes (bytes per sub-block):

| 00 | 01 | 02 | 03 | 04 | 05 | 06 | 07 | 08 | 09 | 0A | 0B | 0C | 0D |
|----|----|----|----|----|----|----|----|----|----|----|----|----|----|
| 66 |  8 | 13 |  6 | 10 |  4 |  7 |  5 |  5 | 77 | 21 |  9 |  5 |  9 |

(= 245 bytes/patch.) This is the user's own patch data, not vendor material, and
may be removed once the typed model is validated.
