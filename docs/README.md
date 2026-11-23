# QSMxT documentation site

The site at https://qsmxt.github.io/QSMxT/ is built with [Astro Starlight](https://starlight.astro.build)
from this directory. `.github/workflows/docs.yml` deploys it on every push to `main` that touches
`docs/`, and builds it (without deploying) on pull requests.

```sh
npm ci
npm run build     # writes dist/; fails on a broken internal link or #heading
npm run dev       # local server at http://localhost:4321/QSMxT/
```

## Layout

- `src/content/docs/`: one Markdown page per URL, in six groups: `start/`, `how-to/`, `data/`,
  `toolbox-pipelines/`, `reference/` and `background/` (plus `index.mdx`, the home page).
- `astro.config.mjs`: the sidebar, the site's head scripts, the link checker
  ([starlight-links-validator](https://starlight-links-validator.vercel.app)) and the redirects.
- `redirects.mjs`: every old page URL and where its content went. Moving or renaming a page, or
  moving a section to another page, needs an entry here, so that links from papers, forum posts
  and older releases keep working, `#heading` included.
- `legacy/`: the QSMxT 8.x site (a Jekyll site on the `python-legacy` branch), which the deploy
  copies into `dist/v8/`. To build it locally: `bundle install` in `legacy/`, then
  `legacy/build.sh <python-legacy checkout> dist/v8`.

## Generated parts

Don't edit these by hand; the tests named fail until they are regenerated and committed.

| What | From | Regenerate |
|---|---|---|
| `reference/cli.md` (whole page) | the `qsmxt` binary's argument definitions (`src/cli_reference.rs`) | `QSMXT_UPDATE_DOCS=1 cargo test --bin qsmxt cli_reference` |
| The `settings` block in `reference/configuration.md` | the settings registry | `QSMXT_UPDATE_DOCS=1 cargo test -p qsmxt-engine reference_docs` |
| The `*-methods` tables in `reference/methods.md` and `reference/source-separation.md`, and the algorithm tables in the repository's `README.md` | QSM.rs's method catalogue and the settings registry (`crates/qsmxt-engine/src/reference_docs.rs`) | the same |

A generated block sits between `<!-- generated:NAME start … -->` and `<!-- generated:NAME end -->`;
the prose around it is hand-written. `src/cli_reference.rs` also checks the hand-written pages:
every `--flag` they name exists, every command is in `reference/commands.md` and
`reference/tools.md`, every algorithm value is described in the method pages, and every output
file is in `reference/outputs.md`.
