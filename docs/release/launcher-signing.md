# Launcher release signing

Status: the workflow exists and skips its build until the owner adds the secrets (#86). The updater is wired (#88; install on Quit #113). The #89 gate passed, so every signed build sets `VITE_SELF_UPDATE=1` and has Check for updates (#157). Nothing here has been run with a real key.

Policy comes from the [#8 resolution](https://github.com/synpoox/resurrected-launcher/issues/8) (slice 5, launcher self-updates) and [ADR 0003](../adr/0003-tauri-with-signed-launcher-updates.md). Launcher updates use Tauri's key-based updater signatures. These signatures are not Authenticode, so the launcher must not claim a verified Windows publisher. Product releases stay unsigned (HTTPS and hashes, `docs/specs/release-manifest.md`), and their feed never supplies a launcher key.

## What the workflow does

`.github/workflows/release.yml` runs on a pushed tag `launcher-v<version>`, or by hand (workflow_dispatch).

| Job | Runs when | Does |
| --- | --- | --- |
| `lint` | Every trigger, including PRs that touch workflows or this folder | Runs pinned, hash-checked `actionlint` on all workflows, and checks that every secret `release.yml` reads is listed below |
| `signing-gate` | Tag or manual run, never on PRs | Reports whether both secrets exist in the `launcher-release` environment. If they don't, it logs a notice and the build is skipped |
| `build` | Only when `signing-gate` found both secrets | Checks that the tag equals `launcher-v` + `tauri.conf.json` `version`, runs `tauri build` with `createUpdaterArtifacts`, writes `latest.json`, and uploads the installer, its `.sig` and `latest.json` as a workflow artifact kept for 14 days |
| `draft-release` | Only after a successful `build` on a pushed tag `launcher-v<version>`; never for a `-gate` tag, a manual run or a PR | Fails if a release for the tag already exists. Otherwise downloads the artifact and creates a **draft** release titled `Project Resurrected launcher <version>`, with `docs/release/notes/<version>.md` as the body and the installer, its `.sig` and `latest.json` attached under their own names. Needs no secrets and no environment; its token has `contents: write` for this job only |

The workflow creates a draft release (#238) but never publishes one, never marks one latest, and never writes a tag. The owner's one remaining step: open the draft under Releases, check the title, notes and assets, then click **Publish**. Publishing is what makes it the latest release, which the updater endpoint below follows. Promotion and the public test channel belong to slice 6.

`latest.json` points at `https://github.com/<repo>/releases/download/launcher-v<version>/<installer>`. That URL only works once the draft is published. The updater endpoint the app trusts is `plugins.updater.endpoints` in `tauri.conf.json` (`https://github.com/synpoox/resurrected-launcher/releases/latest/download/latest.json`, decided on #86, set by #88), so it only answers once the owner publishes the draft.

The bundle is named after `productName` (`Project Resurrected`, #151), so Tauri writes `Project Resurrected_<version>_x64-setup.exe`. The workflow renames it and its `.sig` to `Project-Resurrected_<version>_x64-setup.exe` before writing `latest.json` (the release and gate URLs both use the renamed file), because GitHub would otherwise rename a spaced asset on upload and the URL would break. The draft uses the renamed files as they are.

What's new (#189): put the launcher's release notes in `docs/release/notes/<version>.md` (the patch-notes Markdown subset) before tagging. The workflow copies them into `latest.json`'s `notes`; after the self-update installs, the launcher shows them once in a What's new dialog. With no file, `notes` is empty and no dialog opens.

## Gate builds (#89)

A tag `launcher-v<version>-gate` (same `tauri.conf.json` version) makes a slice 5 gate build instead. It differs from a release build in one way only, checked by `src-tauri/tests/slice5_gate/gate_build.rs` (both set `VITE_SELF_UPDATE=1`, so both check for updates):
- `tauri build --config src-tauri/gate/tauri.gate.conf.json` sets the updater endpoint to `http://127.0.0.1:8789/latest.json` (with `dangerousInsecureTransportProtocol`, loopback only). The public key, install mode and everything else stay the release config, so every update is still verified against the release key. The config is fixed at build time; nothing at runtime can change it.

Its `latest.json` points at `http://127.0.0.1:8789/<installer>` and the artifact is named `launcher-<version>-gate-unpublished`. `src-tauri/gate/serve-update.ps1` serves one update on that address (normal, `-Tamper`, a wrong-key `.sig`, `-Unsigned`), so N → N+1 and the bad-signature rows run without publishing anything. Never distribute a gate build.

## Secrets

Both secrets live in the GitHub **environment** `launcher-release`, not in repository secrets. Only the `build` step that runs `tauri build` receives them.

| Secret | Value |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | The full contents of the private key file made by `tauri signer generate`. Tauri also accepts a path, but CI has no such file. |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | The key's password. The workflow requires one: an empty password counts as missing. |

No other secret is needed. `GITHUB_TOKEN` is read-only everywhere except `draft-release`, which gets `contents: write` and nothing else. The **public** key is not a secret: it is committed in `tauri.conf.json` as `plugins.updater.pubkey` (#88), and a test pins it to the key posted on #86.

Environment settings:
- Required reviewer: the owner.
- Deployment branches and tags: tags matching `launcher-v*` only. A manual run must then select a `launcher-v*` tag as its ref, or the job is refused.
- If the environment doesn't exist when a tag or manual run starts, GitHub creates it without protection rules. Create it first, before any tag.

## Key generation (owner only, #86)

Do this on a trusted machine that you control, outside any repository checkout. No agent runs these steps or sees their output.

1. In a launcher checkout with `npm ci` done, run:
   ```
   npm run tauri signer generate -- -w <somewhere-outside-the-repo>\resurrected-launcher.key
   ```
   Enter a strong, unique password when asked. This writes `resurrected-launcher.key` (private) and `resurrected-launcher.key.pub` (public).
2. Make the recovery copy now (next section) before you use the key anywhere.
3. In GitHub, go to Settings → Environments → `launcher-release` and add both secrets. Paste the key file's contents, not its path.
4. Post the contents of `resurrected-launcher.key.pub` on #86.
5. Delete any extra plaintext copies, such as downloads, clipboard history or temp files. Keep only the working copy and the recovery copy.

Never paste the private key or its password into an issue, PR, chat, commit, log or the app.

## Recovery copy

Signing-key backup and a tested recovery procedure are slice 5 release gates (#8). If the key is lost, the installed launchers can never accept another update. Players would then have to reinstall by hand from the standalone installer.

**Store**
- Keep one encrypted copy of the private key file, separate from the working copy. An example is a password-manager attachment, or an encrypted archive on offline media kept in a different place.
- Keep the key password and the recovery copy's encryption password apart from the copy itself, for example in a password manager the media doesn't depend on.
- Write down where each piece is, without the secrets, somewhere the owner will find it.

**Restore drill** (once now, then after any change)
1. On a different machine, decrypt the recovery copy into a scratch folder outside any repository.
2. Sign a scratch file with it: `npm run tauri signer sign -- -f <restored.key> <scratch-file>`. Set the password in `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` for that shell only, or pass it with `-p` and then clear the shell history. This proves the copy decrypts and the password is right.
3. The #89 gate checks this properly: an installed launcher must accept an update signed with the restored key.
4. Delete the restored key and the scratch files. Record the drill date on #86.

**Rotation:** ship a launcher release, signed with the current key, whose `tauri.conf.json` carries the new public key. Only after that release is installed widely do you start signing with the new key. Never accept a new key from the unsigned product feed.

**Lost key:** recovery may require a deliberate manual reinstall with a new key. Tell players, and publish the standalone installer.

**Compromised key:** stop any release promotion at once and remove the secrets from `launcher-release`. Then rotate if the old key is still trusted. Otherwise follow the lost-key path. Record what happened on the tracker.

## Standalone recovery installer

The NSIS `*-setup.exe` that the workflow uploads is also the standalone recovery installer. It installs per user (`installMode: currentUser`) and includes the offline WebView2 installer. Keep every released version's installer downloadable. Automatic launcher rollback is not assumed (#8).

## Clean-VM test matrix (#87 provides the machines, #89 records the results)

Hosted CI does not prove this matrix. Use clean machines and revert to a snapshot before each row. Run as a **standard user** unless a row says otherwise.

| # | Check | Windows 10 22H2 x64 | Windows 11 x64 |
| --- | --- | --- | --- |
| 1 | Clean per-user install from `*-setup.exe`: no UAC prompt, the launcher starts and finds its bundled 7-Zip | required | required |
| 2 | No WebView2 Runtime, network off: the offline WebView2 installer runs and the launcher starts | required (no-WebView2 snapshot) | if a snapshot exists |
| 3 | Normal-user permissions: the launcher never asks for elevation, and refused writes are explained | required | required |
| 4 | Upgrade N → N+1 through the updater: settings, the D2R location and installed-release records survive | required | required |
| 5 | Bad signature: a tampered installer, a wrong-key `.sig` and an unsigned artifact are each rejected, and N keeps running | required | required |
| 6 | Interrupted update: kill the updater, and separately power off during install. Afterwards N or N+1 works, or the recovery installer restores it with player data intact | required | required |
| 7 | Restart waits while a game or product operation runs (held phases), then completes | required | required |
| 7a | Quit with an update ready installs it without relaunching; Quit while a game runs exits without installing, and a later Quit installs it (#113) | required | required |
| 8 | Key recovery: N accepts an update signed with the key restored from the recovery copy | once, on either machine | |
| 9 | Uninstall leaves player files and D2R untouched | required | required |

For each run, record the exact Windows build, launcher versions N and N+1, the WebView2 version and the workflow run URL.

## Not covered

- Authenticode signing: deferred (#8). SmartScreen warnings are expected.
- Publishing, the public test channel and promotion: slice 6.
