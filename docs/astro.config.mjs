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
						{ label: 'Algorithms', slug: 'reference/algorithms' },
						{ label: 'Configuration', slug: 'reference/configuration' },
						{ label: 'Standalone tools', slug: 'reference/tools' },
					],
				},
			],
		}),
	],
});
