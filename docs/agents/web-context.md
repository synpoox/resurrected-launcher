# Shared website decisions

The related website is maintained in [synpoox/project-web-clean](https://github.com/synpoox/project-web-clean). Its [agent context](https://github.com/synpoox/project-web-clean/blob/main/docs/agents/project-context.md) is the entry point for approved web decisions and unresolved launch inputs.

## Decisions relevant to launcher agents

- Latest website display name: **Project Diablo 2: Resurrected** (older planning: PD2: Resurrected / Project Diablo II: Resurrected). Existing PD2R shorthand and technical paths do not establish display copy. Do not use PD2Resurrect as a public name.
- The owner wants the website and launcher to feel related. The shared inspiration is [Riot Games — Game Launcher — UI/UX Design by Juan Andrés Cutillas](https://www.behance.net/gallery/90336771/Riot-Games-Game-Launcher-UIUX-Design).
- The website currently defaults to the launcher-inspired treatment and retains the darker D2R alternative. This default remains reversible; neither theme settles final shared branding. See [web ADR 0002](https://github.com/synpoox/project-web-clean/blob/main/docs/adr/0002-shared-design-and-switchable-themes.md).
- Web-specific reviewed choices include top navigation, one hero sentence and official D2R Warlock artwork. They do not prescribe the launcher's navigation or module structure.
- The first web release is a static page featuring the playable product and installation link, with no account, game connection or backend. See [web ADR 0001](https://github.com/synpoox/project-web-clean/blob/main/docs/adr/0001-static-public-launch.md) and [launch spec](https://github.com/synpoox/project-web-clean/issues/1).
- projectd2r.com is the planned domain. Branding is provisional and public hosting remains deferred. The owner supplied [Discord](https://discord.gg/mGpHT26Xak), reused from [web PR #3](https://github.com/synpoox/project-web-clean/pull/3); its server name does not settle the umbrella identity.

The launcher glossary and ADRs remain authoritative for launcher architecture, project accounts, installation/update behavior, hosted inventory authority and plugins. The website's small first release does not supersede those decisions, and future launcher capabilities do not expand the web launch automatically. No shared UI package or web/launcher API has been approved.

When a change affects both repositories, link the decision and its issue/PR from both agent contexts. Update the authoritative ADR rather than duplicating a full contract, and identify any superseded decision explicitly.

Implementation and owner approval: [project-web PR #2](https://github.com/synpoox/project-web-clean/issues/2). This backlink records the shared decisions; it does not add launcher functionality.

## Static player guides and unresolved naming

[Web PR #4](https://github.com/synpoox/project-web-clean/pull/4) extends the static home with Getting Started, FAQ and a D2R Tools directory, reusing open Discord PR #3. [Web ADR 0003](https://github.com/synpoox/project-web-clean/blob/codex/guides-tools-naming/docs/adr/0003-static-player-guides-and-tools-directory.md) expands web ADR 0001's page count/directory scope only. It preserves web ADR 0002's shared layout, launcher default and D2R alternative. Public hosting/domain setup stays deferred.

The website directory provides ordinary documentation/release links and optional browser-local filters. It is not a launcher plugin catalog or installer. [Launcher ADR 0004](../adr/0004-manual-plugins-allowed-in-direct-coop.md) still owns the distinction between manual optional plugins and approved release-managed extensions, including the exclusion from project-hosted multiplayer and ladder. General plugin TCP/IP capability does not make the current PD2: Resurrected offline alpha multiplayer-ready. The backup guidance does not alter [launcher ADR 0001](../adr/0001-warn-on-save-breaking-updates-without-forced-isolation.md) or promise conversion/save compatibility. No launcher contract or implementation changes are proposed.

## Current owner review: provisional identity and whole-project scope

The website covers all four projects. Latest display names/order are **Project Diablo 2: Resurrected**, **Diablo II: Resurrected Forever**, **Diablo II: Resurrected 09**, and **D2R Tools**. **Project Resurrected** remains the provisional umbrella; **The Resurrected Project** is another owner suggestion. Branding is not final. [Web ADR 0004](https://github.com/synpoox/project-web-clean/blob/codex/guides-tools-naming/docs/adr/0004-provisional-umbrella-and-project-directory.md) and web `branding.json` own the current labels. Earlier Project Resurrected: 09 / Forever labels are historical. Technical repositories, installed paths, releases and launcher contracts remain unchanged.

The website hero rotates only the three mods, ordered PD2R → Forever → 09, using distinct official D2R promotional artwork. Tools has compact 3/2/1-column plugin cards; Getting Started remains one bounded reading column. Its tallest-panel experiment was rejected and reverted. The owner selected a compact step navigator with Next buttons in the continuation; the resulting layout remains subject to visual/keyboard review. The authoritative detail and QA limits live in web ADR 0004 and its verification record. These web choices do not prescribe launcher navigation or implement launcher features.

ADR 0004 supersedes the earlier web restriction against a project directory and records subsequent display/layout directions. 09/Forever had title-only READMEs and no releases at the recorded check; planning descriptions are not shipped-feature evidence. No launcher ADR is superseded. Continue review in [web PR #4](https://github.com/synpoox/project-web-clean/pull/4) and [launcher PR #11](https://github.com/synpoox/launcher-clean/pull/11).
