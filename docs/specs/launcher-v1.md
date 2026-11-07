# Launcher v1 specification

Status: planning complete; implementation has not started. The canonical decision index is the [launcher map](https://github.com/synpoox/resurrected-launcher/issues/2). Detailed answers live in the linked resolution comments, not duplicated in this index.

## Read in order

1. [First-release journeys and acceptance boundaries](https://github.com/synpoox/resurrected-launcher/issues/5).
2. [Release manifest and safe-update contract](https://github.com/synpoox/resurrected-launcher/issues/6).
3. [Accounts and online-service contracts](https://github.com/synpoox/resurrected-launcher/issues/7).
4. [Architecture, explicit policy revisions and phased implementation order](https://github.com/synpoox/resurrected-launcher/issues/8).

The architecture resolution records later choices that supersede earlier proposals: a tray presence is required; optional game plugins are manually installed and allowed in direct TCP/IP co-op; Tauri artifact signatures are required for launcher updates, while signed game-release metadata and paid Windows publisher certificates remain deferred. Use those explicit revisions when reading earlier contracts.

## Supporting evidence and vocabulary

- [D2R installation, launch and save-isolation research](https://github.com/synpoox/resurrected-launcher/blob/migration-evidence/002/docs/research/d2r-launcher-constraints.md). Read its limitations and the later installer-availability correction in the release/update resolution.
- [Project glossary](../../GLOSSARY.md).
- [Save-breaking releases](../adr/0001-warn-on-save-breaking-updates-without-forced-isolation.md).
- [Authoritative hosted inventory](../adr/0002-hosted-services-own-tradeable-inventory.md).
- [Tauri and signed launcher updates](../adr/0003-tauri-with-signed-launcher-updates.md).
- [Manual plugins and direct co-op](../adr/0004-manual-plugins-allowed-in-direct-coop.md).

## Handoff

Start with contract/packager fixtures and native recovery tests on disposable installations, following the architecture resolution's ordered slices. Actual game writes, public installer promotion and online capability enablement each have separate acceptance gates there. Do not treat planning completion as evidence that any runtime, installer, archive, recovery or hosted-enforcement check has passed.

Game content, website/backend implementation and detailed asynchronous settlement remain outside this repo's launcher implementation scope. No old launcher code is to be reused.
