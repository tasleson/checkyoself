# 🕵️‍♂️ CheckYoSelf

## What?

A no-nonsense (okay, *some* nonsense) utility that recursively walks through a directory and records each file’s:

- 🧬 `blake3` hash
- 📏 file size
- 🕰️ last modified time (in epoch seconds, because... computers)

All of this gets saved into a tidy little JSON file we like to call... *evidence*.


## Why?

Upgraded to Fedora 42, suddenly `btrfs` decided to gaslight me with checksum errors.
Instead of spiraling into existential doubt, I made this tool to prove my files aren’t lying to me. (Or at least to catch them red-handed.)


## How?

On its first run, CheckYoSelf scans your files and saves their vital stats to JSON.
Later, you can verify if any of them have mysteriously mutated—**but only if the modified time hasn’t changed** (because we're not psychic).

You can even:
- 🛠️ Update the JSON when new files appear
- 📦 Detect file moves
- 🫣 Quiet the noise with `--q`


### 🧪 Usage

```bash
checkyoself <directory> <output.json> [--progress] [--skip <dir>...] [--q]

checkyoself <directory> --verify <ref.json> [--update] [--progress] [--skip <dir>...] [--q]

```

### 🧹 Options

`--verify` Compare the JSON file to what the directory currently has.

`--update` Update the JSON file to reflect recent changes.

`--progress` Displays a simple moving bar to give you an idea how long it will take.

` --skip <directory name> ` Skips over any directories with that name. Repeat it as much as you want. (--skip node_modules recommended for your sanity.)

`--q` Shhh... suppresses all output except for mismatches. Great for scripting or dramatic tension.

### ✅ Exit Codes

* 0: All good
* Anything else: Something smells fishy 🐟

### 🚨 Disclaimer

This tool is held together by hope and hash functions.

It:

* 🧨 Might corrupt your data
* 🐕 Might kick your dog
* 🐈 Might spook your cat
* 🧘 Might give you a false sense of control

#### Use at your own risk. You’ve been warned.
