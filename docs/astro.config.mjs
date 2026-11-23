// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import starlightLinksValidator from 'starlight-links-validator';
import { readFile, writeFile } from 'node:fs/promises';
import { moved, split, v8 } from './redirects.mjs';

const base = '/QSMxT';

// Every old page URL redirects to where its content is now (see redirects.mjs).
const redirects = Object.fromEntries([
	...Object.entries(moved).map(([from, { to }]) => [from, base + to]),
	...v8.map((from) => [from, `${base}/v8${from}`]),
]);

// The script a page runs to send a link to one of its old headings on to the page that heading
// moved to. `fallback` is where to go when the hash is not in `anchors` (null: stay).
function hashForwarder(anchors, fallback) {
	const map = Object.fromEntries(Object.entries(anchors).map(([a, to]) => [a, base + to]));
	return (
		`(function(){var m=${JSON.stringify(map)},f=${JSON.stringify(fallback)},h=location.hash.slice(1);` +
		'try{h=decodeURIComponent(h)}catch(e){}' +
		'var t=m[h]||(f&&f+location.hash);if(t)location.replace(t);})();'
	);
}

// Astro's redirect pages are a bare meta refresh, which drops the `#heading` of the link. Give
// each one a script that keeps it, and follows a heading that moved to another page.
const redirectAnchors = {
	name: 'qsmxt-redirect-anchors',
	hooks: {
		'astro:build:done': async ({ dir }) => {
			const pages = [
				...Object.entries(moved).map(([from, { to, anchors }]) => [from, hashForwarder(anchors, base + to)]),
				...v8.map((from) => [from, hashForwarder({}, `${base}/v8${from}`)]),
			];
			for (const [from, script] of pages) {
				const file = new URL(`.${from.replace(/\/?$/, '/')}index.html`, dir);
				const html = await readFile(file, 'utf8');
				if (!html.includes('http-equiv="refresh"') || !html.includes('<title>')) {
					throw new Error(`${from} is not a redirect page Astro writes`);
				}
				await writeFile(file, html.replace('<title>', `<script>${script}</script><title>`));
			}
		},
	},
};

// https://astro.build/config
export default defineConfig({
	// GitHub Pages project site for QSMxT/QSMxT. Override `site`/`base` here if a
	// custom domain (e.g. qsmxt.org) is configured.
	site: 'https://qsmxt.github.io',
	base,
	redirects,
	integrations: [
		redirectAnchors,
		starlight({
			title: 'QSMxT',
			description:
				'A fast, BIDS-native Quantitative Susceptibility Mapping pipeline and interactive TUI, built in Rust.',
			logo: {
				src: './src/assets/logo.svg',
				replacesTitle: false,
			},
			favicon: '/favicon.svg',
			customCss: ['./src/styles/theme.css'],
			// Cross-project "ecosystem bar" shared across all QSM sites
			// (canonical source: QSMxT/qsmxt.github.io → qsm-nav.js).
			head: [
				...Object.entries(split).map(([page, { anchors }]) => ({
					tag: 'script',
					content: `if(location.pathname===${JSON.stringify(base + page)})${hashForwarder(anchors, null)}`,
				})),
				// Actually load Inter + JetBrains Mono (the theme only *names* them);
				// without this, Starlight falls back to a system font and looks unlike
				// the rest of the QSM family.
				{ tag: 'link', attrs: { rel: 'preconnect', href: 'https://fonts.googleapis.com' } },
				{ tag: 'link', attrs: { rel: 'preconnect', href: 'https://fonts.gstatic.com', crossorigin: true } },
				{
					tag: 'link',
					attrs: {
						rel: 'stylesheet',
						href: 'https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500&display=swap',
					},
				},
				{
					// Dark-first, to match the rest of the QSM family (users can still toggle).
					tag: 'script',
					content:
						"(function(){try{if(!localStorage.getItem('starlight-theme')){" +
						"localStorage.setItem('starlight-theme','dark');" +
						"document.documentElement.dataset.theme='dark';}}catch(e){" +
						"document.documentElement.dataset.theme='dark';}})();",
				},
				{
					// Preselect the install tab matching the visitor's OS. Starlight's
					// synced tabs restore from this localStorage key while the page
					// parses, so seeding it here avoids a flash of the wrong command.
					// Only seeded when unset, so an explicit tab click still wins.
					tag: 'script',
					content:
						"(function(){try{var k='starlight-synced-tabs__os';" +
						"if(localStorage.getItem(k))return;" +
						"var d=navigator.userAgentData,p=(d&&d.platform)||navigator.platform||'';" +
						"var win=/win/i.test(p)||/Windows/i.test(navigator.userAgent||'');" +
						"localStorage.setItem(k,win?'Windows install':'macOS / Linux install');}catch(e){}})();",
				},
				{
					tag: 'script',
					content:
						"(function(){var hosted='https://qsmxt.github.io/qsm-nav.js';" +
						"var local=location.hostname==='localhost'||location.hostname==='127.0.0.1';" +
						"function load(src,onfail){var s=document.createElement('script');" +
						"s.src=src;s.dataset.current='xt';s.onerror=onfail||null;" +
						"document.head.appendChild(s);}" +
						// Locally, prefer a checked-out copy of the bar so it can be edited
						// alongside the site — but fall back to the hosted one when there
						// isn't one, so dev doesn't render a headerless dead strip.
						"if(local){load('/qsm-nav.js',function(){load(hosted);});}" +
						"else{load(hosted);}})();",
				},
			],
			social: [
				{ icon: 'github', label: 'GitHub', href: 'https://github.com/QSMxT/QSMxT' },
			],
			editLink: {
				baseUrl: 'https://github.com/QSMxT/QSMxT/edit/main/docs/',
			},
			lastUpdated: true,
			plugins: [
				// Fails the build on a broken internal link or #heading. The 8.x site is copied
				// into dist/v8 after this build (legacy/build.sh), so links into it can't be checked.
				starlightLinksValidator({ exclude: [`${base}/v8/`, `${base}/v8/**`] }),
			],
			sidebar: [
				{
					label: 'Start',
					items: [
						{ label: 'Home', link: '/' },
						{ label: 'Install', slug: 'start/install' },
						{ label: 'Tutorial: the TUI', slug: 'start/tutorial-tui' },
						{ label: 'Tutorial: the command line', slug: 'start/tutorial-cli' },
						{ label: 'Coming from QSMxT 8.x', slug: 'start/coming-from-8x' },
					],
				},
				{
					label: 'How-to guides',
					items: [
						{ label: 'Convert DICOM to BIDS', slug: 'how-to/convert-dicom' },
						{ label: 'Prepare your data', slug: 'how-to/prepare-data' },
						{ label: 'Run on a BIDS dataset', slug: 'how-to/run-bids' },
						{ label: 'Choose methods', slug: 'how-to/choose-methods' },
						{ label: 'Build a custom mask', slug: 'how-to/build-mask' },
						{ label: 'Source separation', slug: 'how-to/source-separation' },
						{ label: 'Multi-orientation', slug: 'how-to/multi-orientation' },
						{ label: 'Referencing and statistics', slug: 'how-to/reference-and-stats' },
						{ label: 'Run on a cluster (SLURM)', slug: 'how-to/slurm' },
						{ label: 'Export results as DICOM', slug: 'how-to/export-dicom' },
						{ label: 'Update QSMxT', slug: 'how-to/update' },
					],
				},
				{
					label: 'Toolbox pipelines',
					// The overview and one generated page per pipeline, in QSM.rs's order
					// (each page's `sidebar.order`).
					items: [{ autogenerate: { directory: 'toolbox-pipelines' } }],
				},
				{
					label: 'Reference',
					items: [
						{ label: 'Methods index', slug: 'reference/methods' },
						{ label: 'Settings', slug: 'reference/configuration' },
						{ label: 'Commands', slug: 'reference/commands' },
						{ label: 'Command line', slug: 'reference/cli' },
						{ label: 'Standalone tools', slug: 'reference/tools' },
						{ label: 'Input data layout', slug: 'reference/inputs' },
						{ label: 'Output files', slug: 'reference/outputs' },
					],
				},
				{
					label: 'Background',
					items: [
						{ label: 'How the pipeline works', slug: 'background/pipeline' },
						{ label: 'Recommendations', slug: 'background/recommendations' },
						{ label: 'Geometry', slug: 'background/geometry' },
						{ label: 'Using the TUI', slug: 'background/tui' },
					],
				},
			],
		}),
	],
});
