# Shared website decisions

The related website is maintained in [synpoox/project-web-clean](https://github.com/synpoox/project-web-clean). Its [agent context](https://github.com/synpoox/project-web-clean/blob/main/docs/agents/project-context.md) is the entry point for approved web decisions and unresolved launch inputs.

## Decisions relevant to launcher agents

- Product display name: **PD2: Resurrected**, or **Project Diablo II: Resurrected** in full. Existing PD2R shorthand and technical paths do not establish display copy. Do not use PD2Resurrect as a public name.
- The owner wants the website and launcher to feel related. The shared inspiration is [Riot Games — Game Launcher — UI/UX Design by Juan Andrés Cutillas](https://www.behance.net/gallery/90336771/Riot-Games-Game-Launcher-UIUX-Design).
- The website currently defaults to the launcher-inspired treatment and retains the darker D2R alternative. This default remains reversible; neither theme settles final shared branding. See [web ADR 0002](https://github.com/synpoox/project-web-clean/blob/main/docs/adr/0002-shared-design-and-switchable-themes.md).
- Web-specific reviewed choices include top navigation, one hero sentence and official D2R Warlock artwork. They do not prescribe the launcher's navigation or module structure.
- The first web release is a static page featuring the playable product and installation link, with no account, game connection or backend. See [web ADR 0001](https://github.com/synpoox/project-web-clean/blob/main/docs/adr/0001-static-public-launch.md) and [launch spec](https://github.com/synpoox/project-web-clean/issues/1).
- projectd2r.com is the planned domain. The shared display name, community destination and public hosting remain unresolved; do not invent them.

The launcher glossary and ADRs remain authoritative for launcher architecture, project accounts, installation/update behavior, hosted inventory authority and plugins. The website's small first release does not supersede those decisions, and future launcher capabilities do not expand the web launch automatically. No shared UI package or web/launcher API has been approved.

When a change affects both repositories, link the decision and its issue/PR from both agent contexts. Update the authoritative ADR rather than duplicating a full contract, and identify any superseded decision explicitly.

Implementation and owner approval: [project-web PR #2](https://github.com/synpoox/project-web-clean/issues/2). This backlink records the shared decisions; it does not add launcher functionality.
