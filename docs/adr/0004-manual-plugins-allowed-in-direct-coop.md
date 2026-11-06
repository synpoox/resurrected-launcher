# Allow manual player plugins in offline play and direct co-op

Players install optional product-local game plugins manually; the launcher provides no plugin catalog or installer. The owner explicitly allows these plugins in offline single-player and direct TCP/IP co-op, but excludes them from project-hosted multiplayer and ladder play; approved release-managed mod plugins are a separate category. This supersedes the broader offline-only wording discussed earlier, and launcher preflight checks do not establish authoritative server enforcement.

See the [architecture resolution](https://github.com/synpoox/launcher-clean/issues/8) for inventory checks and the external hosted-enforcement gate.
