# Nana profiles

Some KiriKiri Z titles hide an XP3 filename table behind values the game derives
from a short startup string. CrySirror reads those values from companions placed
beside the XP3 archives:
`crysirror-nana.local.json`, `content-table.bin` and `<executable stem>.key`.
The runtime path is documented in [docs/RESOURCES.md](../../docs/RESOURCES.md);
the decoder and its validation live in
[the Nana startup record](../validation/krkr-nana-shrine-windows.md).

## What lives here

| Path | Contents |
| --- | --- |
| [`example/`](example/profile.json) | A self-consistent placeholder bundle (no game material) |
| [the installer](../../tools/install_nana_profile.py) | Verification, derivation, installation and packaging |

A profile is **user data, not a repository asset**. The values are derived from
one installation's own startup string, so they belong to whoever owns that
installation. This directory keeps the format, the validator and a placeholder
bundle that exercises both; recovered profiles stay outside the repository.

The placeholder bundle uses the fixed startup line `CrySirror-Example-Key` and
generic content parameters. `--check` re-derives the control table, mask,
offset and all six name-state words from that key, so the bundle verifies — but
installing it into a real game directory does not unlock anything. Generate a
bundle from your own installation instead.

## Generate a bundle from an installation

The startup key is the input. Write it as a UTF-16LE file (one line, optionally
with a BOM), then derive and package:

```sh
python tools/install_nana_profile.py --derive "D:\\game\\localove3.key"
python tools/install_nana_profile.py --profile example --game-dir "D:\\game"
python tools/install_nana_profile.py --profile example --zip nana-transfer.zip
```

`--derive KEY` prints the control block prefix, the two filter parameters and
the six name-state words that the key produces, which is the starting point for
a new profile directory: put the printed values into
`crysirror-nana.local.json`, write the 4,096-byte control block to
`content-table.bin`, and describe both in `profile.json`.

## Install and verify

```sh
python tools/install_nana_profile.py --list
python tools/install_nana_profile.py --profile example --check
python tools/install_nana_profile.py --profile example --game-dir "D:\\games\\loca-love"
```

The installer verifies the digests, re-derives the control block, the two
content-filter parameters and the six name-state words from the startup key, and
only then copies the companions next to the selected executable. It renames the
key to `<executable stem>.key`, skips files that already match, and refuses to
replace a differing file without `--force`.

Move a bundle between machines (PC to PC, or PC to a CrySirror mobile build)
with `--zip`; the archive carries the same three companions under their
destination names.

## What the bundle does not do

The launcher still needs the three files on disk. Starting from a clean
installation without them would require the archive reader to serve the profile
and control block from bundled data through the companion resolver, and to obtain
a verifiable startup line from the native executable. Neither path exists yet;
see the clean-install section of
[krkr-nana-profile-bundle.md](../validation/krkr-nana-profile-bundle.md).
