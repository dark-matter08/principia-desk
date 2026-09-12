# principia.ndelucien.com

The landing page for Principia Desk. Astro, static output, no client-side framework, no font CDN, no analytics: the page makes no request to any origin but its own.

```bash
cd landing
npm install
npm run dev        # http://localhost:4321
npm run build      # → dist/
npm run check      # astro check
```

## Where the content lives

All of it is in **`src/content/site.json`**, validated by the Zod schema in `src/lib/schema.ts`. No copy is written into a `.astro` file; the page is a walk over the schema, so editing the site is editing that JSON. The changelog page reads `../CHANGELOG.md` and the licence page `../LICENSE` at build time, so a release and its notes are one commit.

Screenshots in `public/shots/` are taken from the desk's browser preview (`npm run dev` at the repository root) at 1440×900. The share card `public/og.png` is rendered from `scripts/og.html` with headless Chrome:

```bash
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless=new --disable-gpu \
  --window-size=1200,630 --screenshot=public/og.png "file://$PWD/scripts/og.html"
```

## Search engines

Every page has a title, a description, a canonical URL, Open Graph and Twitter cards, and the home page carries `SoftwareApplication` and `FAQPage` structured data. `@astrojs/sitemap` writes `sitemap-index.xml`, which `robots.txt` names. Unknown paths answer 404, not the home page.

## Deploying

See [docs/DEPLOY.md](docs/DEPLOY.md). Short version: the Landing CI workflow builds and pushes an image on every change to `landing/` on `main`, and the Deploy Landing workflow ships it to the shared box as the `principia` stack behind the box's nginx. `make -C landing deploy-production` does the same by hand.
