# Nana profile derivation for a KiriKiri Z installation

2026-11-25. A Nana profile was recovered for ろけらぶ 神社×先輩 with
`localove3.exe`, and the derivation it depends on was verified against an
independent implementation. The recovered values belong to that installation's
own copy, so this record keeps the derivation, the verification numbers and the
install path, and no longer ships the companions themselves. The repository holds
a self-consistent placeholder under
[profiles/nana/example](../../profiles/nana/example) that exercises the same
verification.

## The three companions reduce to one startup line

`nana_profile.rs::derive_setup` builds the control block from the startup line
with SHAKE256 over its UTF-16LE bytes and takes the two content-filter
parameters and the six name-state words from SipHash-2-4 keyed by
`control[0:16]`. An independent Python implementation of that derivation was run
against the recovered key and reproduced every stored value:

| Value | Derived from the recovered `.key` | Stored companion |
| --- | --- | --- |
| Startup line length | 16 characters | 34-byte UTF-16LE record with BOM |
| Control block | `e529faf4da794f88…`, 4,096 bytes | `content-table.bin`, byte-for-byte equal |
| `mask` | 384 | 384 |
| `offset` | 168 | 168 |
| `name_state` | `[3500753765, 1543227078, 4225439133, 2717729756, 541954468, 1106711445]` | identical |

The derivation holds no decoded game content: the 4,096-byte table is
reproducible from the 16-character line, and the profile only adds the branch
orderings and the LCG seed. The three recovered companions totalled 4,442 bytes.
The placeholder bundle carries the same shape — a fixed 22-character startup
line, a derived 4,096-byte table and a 275-byte profile — and its manifest lists
the sizes and SHA-256 digests of its own files.

## Where the recovered companions went

The companions were committed with this record on branch
`codex/nana-profile-bundle` and removed from it the same day, because a profile
is derived from one installation and travels with whoever owns that
installation, not with the project. The commit that added them remains in the
branch history; the current tree contains only `profiles/nana/example`, the
installer and this record. Recovering a bundle from an installation is a local
step:

```sh
python tools/install_nana_profile.py --derive "D:\\game\\localove3.key"
```

## Install

`tools/install_nana_profile.py` verifies the digests, re-derives the table and
the eight parameters, then copies the companions beside the selected
executable, renaming the key to `<executable stem>.key` because
`crysirror-launch` looks the key up under the runtime's stem. The file already
matching is left untouched, a differing file is kept unless `--force` is given,
and the run reports the detection the launcher will perform. A second run on an
installed directory copies nothing.

```sh
python tools/install_nana_profile.py --list
python tools/install_nana_profile.py --profile example --check
python tools/install_nana_profile.py --profile example --zip nana-transfer.zip
```

## Companion coverage against startup

| Companions present | Result |
| --- | --- |
| profile and control block | `dls:` table decodes; the launcher selects the runtime; startup reports failure |
| all three | reaches the title screen |

The three-companion case is the 8,800-frame Windows offscreen run with the
first-run language selector accepted and `title.ks` waiting at `*wait`, recorded in
[krkr-nana-shrine-windows.md](krkr-nana-shrine-windows.md). The two-companion
run is the clean-install probe that selected the runtime and then failed before
the title screen.

## What the companions do not do

Installing still means copying three files beside the game. Starting without them
would need the archive reader to serve the profile and the control block from
bundled data through the companion resolver, and the launcher to obtain a
verifiable startup line from a clean installation instead of from a recovered
`crysirror-nana.local.json`. Neither path exists: the launcher accepts a
profile only from the game's asset scope, and the native bootstrap still reads
its key file through `Array.load`. Existing support therefore begins at a
recovered local profile.
