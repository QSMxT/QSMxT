import { defineCollection, z } from 'astro:content';
import { docsLoader } from '@astrojs/starlight/loaders';
import { docsSchema } from '@astrojs/starlight/schema';

export const collections = {
	docs: defineCollection({
		loader: docsLoader(),
		schema: docsSchema({
			extend: z.object({
				// Site-wide temporary notice for users arriving from the Python (8.x)
				// version. Pages can override `banner` in their own frontmatter.
				banner: z
					.object({ content: z.string() })
					.default({
						content:
							'QSMxT v9 is a ground-up rewrite in <strong>Rust</strong>. Coming from the Python version (8.x)? <a href="/QSMxT/#coming-from-qsmxt-8x">See what changed →</a>',
					}),
			}),
		}),
	}),
};
