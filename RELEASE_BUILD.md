# Building a Windows release

Dungeon Sandbox ships as a plain folder (zipped): the executable, the game
content and a short guide. No installer, no code signing, nothing uploaded.

## Package locally

Requirements: Windows 10/11 x64, Rust stable (1.97 or newer, via
[rustup](https://rustup.rs)) with the MSVC toolchain, PowerShell 5.1+.

```powershell
powershell -ExecutionPolicy Bypass -File scripts\package-windows.ps1 -Build
```

`-Build` runs `cargo build --release -p dungeon_game -p dungeon_sim` first;
leave it out to package an existing `target\release` build. `-OutDir <path>`
changes the output folder (default `dist\`, which git ignores).

The script:

1. reads the version from `[workspace.package]` in `Cargo.toml`;
2. recreates `dist\DungeonSandbox-<version>-windows-x64\` (only that version's
   folder and ZIP are deleted);
3. copies `dungeon-sandbox.exe`, `dungeon-headless.exe` (if built),
   `HOW_TO_PLAY.txt` (from `packaging\`), `README.md`, `CHANGELOG.md` and the
   `*.toml` files under `assets\content\` — never source, `target\`, logs,
   saves, screenshots, PDBs or custom editor copies (`90_*.toml`);
4. checks the required files exist, at least one dungeon file is present and
   nothing forbidden slipped in, failing with a message that says what to do;
5. writes `dist\DungeonSandbox-<version>-windows-x64.zip` and prints both
   sizes.

Package layout:

```
DungeonSandbox-0.5.0-windows-x64/
  dungeon-sandbox.exe      the game
  dungeon-headless.exe     optional AI-only simulation runner
  HOW_TO_PLAY.txt          player guide (controls, loop, Cinderdeep notes)
  README.md, CHANGELOG.md
  assets/content/dungeons/*.toml, assets/content/variants/*.toml
  saves/                   created on first save (not shipped)
```

## How the packaged build finds its files

* Content: `assets/content/dungeons` relative to the working directory, or
  else next to the executable, so double-clicking, shortcuts and starting it
  from another folder all work. `DUNGEON_CONTENT` overrides the location.
* Saves: the game switches to its own folder at start when the content is
  only found there, so `saves/profile.json` is created next to the exe.
  `DUNGEON_SAVE_DIR` overrides the location.

## Verified (0.5, 2026-10-15)

On the development machine (Windows 11, release build):

* `package-windows.ps1 -Build` succeeded: folder 90.9 MB, ZIP 28.6 MB, 6
  dungeon files.
* The ZIP was extracted to a scratch folder and both executables were started
  **from a different working directory** with `DUNGEON_SAVE_DIR` unset:
  * `dungeon-headless.exe --dungeon cinderdeep_hollows --runs 2`: 2/2
    victories, deterministic replay OK;
  * `dungeon-sandbox.exe --smoke-test 8`: window opened, 465 frames, exit 0,
    logged `using game folder ...`;
  * `dungeon-sandbox.exe --demo --dungeon sunken_vault --speed 10
    --smoke-test 45`: Victory, exit 0, and `saves\profile.json` was created
    inside the extracted folder.

Not verified: other machines, other GPUs, Windows 10, SmartScreen behaviour
for downloaded ZIPs (the build is unsigned; testers may see "Windows protected
your PC" and need *More info → Run anyway*).

## Optional CI artifact

`.github/workflows/windows-package.yml` builds, tests, packages and runs a
headless smoke test on `windows-latest`, then keeps the ZIP as a workflow
artifact for 14 days. It only runs when started by hand (*Actions → Windows
package → Run workflow*), never on push, and never creates a GitHub Release.
It has not been run yet (nothing has been pushed).

## Making a release by hand (when authorised)

1. Bump `version` in `Cargo.toml` `[workspace.package]`, run `cargo check` to
   update `Cargo.lock`, update `CHANGELOG.md`.
2. Run the full checks (README "Development checks") and the package script.
3. Extract the ZIP somewhere else and run the smoke steps above.
4. Upload the ZIP wherever it is meant to go. Publishing is a separate,
   deliberate step; nothing in this repository does it automatically.
