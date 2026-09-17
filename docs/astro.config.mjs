// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://astro.build/config
export default defineConfig({
	// GitHub Pages project site for QSMxT/QSMxT. Override `site`/`base` here if a
	// custom domain (e.g. qsmxt.org) is configured.
	site: 'https://qsmxt.github.io',
	base: '/QSMxT',
	integrations: [
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
			sidebar: [
				{
					label: 'Getting started',
					items: [
						{ label: 'Installation', slug: 'getting-started/installation' },
						{ label: 'Quick start', slug: 'getting-started/quick-start' },
						{ label: 'Coming from QSMxT 8.x', slug: 'getting-started/coming-from-8x' },
					],
				},
				{
					label: 'Guides',
					items: [
						{ label: 'Running interactively', slug: 'guides/running-interactively' },
						{ label: 'Running noninteractively', slug: 'guides/running-noninteractively' },
					],
				},
				{
					label: 'Reference',
					items: [
						{ label: 'Commands', slug: 'reference/commands' },
						{ label: 'Input data', slug: 'reference/inputs' },
						{ label: 'Algorithms', slug: 'reference/algorithms' },
						{ label: 'Configuration', slug: 'reference/configuration' },
						{ label: 'Standalone tools', slug: 'reference/tools' },
					],
				},
			],
		}),
	],
});
