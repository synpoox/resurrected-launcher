# Launcher

The shared launcher for Project Diablo 2: Resurrected, Diablo II: Resurrected Forever, and Diablo II: Resurrected 09. These umbrella/product labels are provisional; technical paths and existing planning aliases remain unchanged.

**Project Resurrected**: The current preview identity for the shared website and project family, including the mods and D2R Tools. The owner also suggested The Resurrected Project and explicitly asked not to finalize branding. [Web ADR 0004](https://github.com/synpoox/resurrected-web/blob/codex/guides-tools-naming/docs/adr/0004-provisional-umbrella-and-project-directory.md) and its `branding.json` record the reversible display-name choice.

**Project Diablo 2: Resurrected** (existing planning alias: **PD2: Resurrected**, **PD2R**):
The latest website preview display name for the product called PD2R or PD2: Resurrected in existing launcher planning. Older planning uses Project Diablo II: Resurrected. Avoid PD2Resurrect in user-facing copy. See the [shared website context](docs/agents/web-context.md).

## Language

**Product**:
One of the three mods offered through the launcher: Project Diablo 2: Resurrected, Diablo II: Resurrected Forever, or Diablo II: Resurrected 09. Existing planning may use PD2R, Project 09, or Project Forever.
_Avoid_: Game mode

**Adoption**:
Recognizing an existing product installation so the launcher can manage and launch it.

**Managed files**:
Files supplied by a product release, distinct from player files and unidentified additions.

**Release**:
An immutable published version of a product, with its own identity and contents.

**Release track**:
A publisher-maintained sequence identifying the recommended release of a product.

**Release manifest**:
A description of a release's contents, compatibility requirements, and file ownership.

**Save-breaking release**:
A release that cannot safely load characters from a previous release's save format.

**Player files**:
A player's saves, edited loot filters, and personal settings associated with a product.

**Shipped defaults**:
Initial configuration supplied with a release, which may subsequently become player files through editing.

**Update recovery**:
Restoring a complete installation after an update fails, without changing or converting player saves.

**Repair**:
Restoring a product's missing or damaged managed files while retaining player files.

**Diablo II: Resurrected 09** (existing planning/repository alias: **Project 09**):
A port of Lord of Destruction 1.09d to Diablo II: Resurrected 3.3.

**Diablo II: Resurrected Forever** (existing planning/repository alias: **Project Forever**):
An overhaul mod of Diablo II: Resurrected 3.3 encompassing class changes, legacy items, league mechanics, and endgame changes.

**TCP/IP co-op**:
Direct multiplayer play without a project-hosted economy or league.

**Hosted multiplayer**:
Play on project-hosted servers with a shared economy and leagues.

**Project account**:
A player's shared identity across the project website, launcher, and account-dependent product services, distinct from Battle.net or Steam identity.

**Marketplace**:
The searchable player-trade listings service used to offer items and arrange exchanges, rather than a project-operated cash shop.

**In-game trade**:
An item exchange completed through the game; the marketplace may help players find and arrange the exchange.

**Economy**:
The boundary within which items and trades are valid, identified by the product and its applicable realm, league, and ruleset.

**Asynchronous trade**:
A fixed-price purchase that can complete without the seller online, with authoritative game services securing the item and settling payment.

**Online capability**:
A product-specific service feature, such as hosted multiplayer or asynchronous trade, whose availability does not establish a player's eligibility to use it.

**Hosted join grant**:
A short-lived permission to enter a hosted game, scoped to a player and a product/economy, distinct from a reusable project-account session.

**Optional game plugin**:
A player-installed product-local game extension, distinct from the mod's approved release-managed extensions; permitted in offline single-player and direct TCP/IP co-op, not project-hosted multiplayer or ladder play.
