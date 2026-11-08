# News feed (`news.json`)

The product page's news feed (bottom right, `docs/specs/launcher-ui.md`) reads one launcher-owned file, `news.json`. Decided by the owner on [#80](https://github.com/synpoox/resurrected-launcher/issues/80):

- It is separate from the release feed. The release feed, schema 3 (`docs/specs/release-manifest.md`) and the d2rAI-toolkit packager are unchanged.
- One file serves every product. The owner publishes it as a release asset of synpoox/resurrected-launcher, so the launcher reads `https://github.com/synpoox/resurrected-launcher/releases/latest/download/news.json` (`app::news::DEFAULT_NEWS_URL`). pd2-resurrected is unchanged.
- There is no cache on disk. A file fetched in this session is reused for 10 minutes. Offline, or a missing or invalid file, reads "News needs a connection. Retry".

## Format

```json
{
  "schema_version": 1,
  "items": [
    {
      "product_id": "pd2r",
      "kind": "announcement",
      "title": "TCP/IP co-op testing weekend",
      "published": "2026-11-01",
      "url": "https://discord.gg/mGpHT26Xak"
    }
  ]
}
```

`fixtures/news/valid.json` is a complete example.

| Field | Rule |
| --- | --- |
| `schema_version` | `1`. Any other value rejects the file. |
| `items` | At most 50 items, across all products and kinds. |
| `product_id` | A catalog product id (`pd2r`, `project-forever`, `project-09`): lowercase letters, digits and `-`, at most 64 characters. Items for other products are ignored. |
| `kind` | `announcement` (tag "Announcement") or `guide` (tag "Guide") are shown. `release_notes` and any other kind are dropped, because Patch Notes owns release notes. |
| `title` | Plain text, 1 to 120 characters after trimming. No control or bidirectional-override characters. It is never parsed as HTML. |
| `published` | `YYYY-MM-DD` or `YYYY-MM-DDTHH:MM:SSZ` (UTC), a real date from 2000 on. |
| `url` | At most 512 characters, `https` on a public host (the release feed's rule: no port, user info, IP literal or dot segment) and the host is exactly one of `github.com`, `discord.gg` or `discord.com`. Add `projectd2r.com` once the website is hosted. |

Unknown fields are ignored. Any other invalid item rejects the whole file. The file is at most 64 KiB; the download stops at the cap.

The feed shows a product's first three items, newest first. The webview receives tag, title and date only. Links stay native: `open_news_link(product_id, index)` opens entry `index` of the last `get_news` reply through `explorer.exe`, after checking the URL against the same rule again. No remote HTML or image is ever rendered, and remote content gets no command or capability.

## "About <product>"

`open_product_page(product_id)` opens a page bundled in `app::news::PRODUCT_PAGES`. It is never fetched. A product shows the button only if it has a page (`aboutPage` in `src/products.ts`, which a test keeps in step with Rust). Today only `pd2r` has one (its public GitHub repo). The button appears only on products without a release, so it is not visible yet. Forever and 09 get a page when their repositories are public.
