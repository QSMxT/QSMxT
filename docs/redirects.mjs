// Old page URLs and where their content went (paths below the site's base, `/QSMxT`).
//
// `moved`: pages that no longer exist. Each becomes a redirect (`redirects` in
// astro.config.mjs) to `to`; a link to one of its headings goes to the page that heading moved to
// (`anchors`, old heading id → new page and id). Headings not listed are on `to` under the same id.
//
// `split`: pages that still exist but gave some sections to other pages; a link to one of those
// sections is sent on to its new page.
//
// `v8`: the QSMxT 8.x site's pages, which moved under /v8/ when v9's site took over the URL.

export const moved = {
	'/getting-started/installation/': {
		to: '/start/install/',
		anchors: {
			'announcements': '/how-to/update/#announcements',
			'keeping-up-to-date': '/how-to/update/',
			'trying-the-main-branch': '/reference/updates/#trying-the-main-branch',
		},
	},
	'/getting-started/quick-start/': {
		to: '/start/tutorial-tui/',
		anchors: {
			'1-convert-dicoms-to-bids': '/start/tutorial-cli/#1-convert-dicoms-to-bids',
			'2-run-the-pipeline': '/start/tutorial-cli/#2-run-the-pipeline',
			'next-steps': '/start/tutorial-cli/#next-steps',
			'the-command-line-way': '/start/tutorial-cli/#the-command-line-way',
			'what-you-get': '/start/tutorial-cli/#what-you-get',
		},
	},
	'/getting-started/coming-from-8x/': {
		to: '/start/coming-from-8x/',
		anchors: {
		},
	},
	'/guides/running-noninteractively/': {
		to: '/how-to/run-qsm/',
		anchors: {
			'a-typical-cluster-workflow': '/how-to/slurm/#steps',
			'caching-plays-nicely-with-retries': '/reference/slurm/#caching-plays-nicely-with-retries',
			'choosing-algorithms': '/how-to/choose-methods/#choosing-algorithms',
			'converting-dicoms-to-bids': '/how-to/convert-dicom/',
			'exporting-results-as-dicom': '/how-to/export-dicom/',
			'how-the-jobs-divide-the-work': '/reference/slurm/#how-the-jobs-divide-the-work',
			'hpc--slurm': '/how-to/slurm/',
			'keeping-the-studys-dicom-uids': '/reference/dicom-conversion/#keeping-the-studys-dicom-uids',
			'output': '/reference/outputs/#output',
			'parallelism-on-a-single-node': '/how-to/slurm/#parallelism-on-a-single-node',
			'pipelines': '/toolbox-pipelines/#pipelines-1',
			'preview-first-with---dry-run': '/how-to/convert-dicom/#preview-first-with---dry-run',
			'recommendations': '/background/recommendations/#recommendations-1',
			'the-reference-summary': '/reference/outputs/#the-reference-summary',
			'the-stages': '/background/pipeline/#the-stages',
			'what-it-handles': '/reference/dicom-conversion/#what-it-handles',
			'multi-echo-phase-handling': '/data/bipolar/',
			're-running': '/how-to/run-qsm/#optional',
			'selecting-runs': '/how-to/run-qsm/#optional',
			'using-a-configuration-file': '/how-to/run-qsm/#optional',
		},
	},
	'/reference/algorithms/': {
		to: '/reference/methods/',
		anchors: {
			'--crop-to-mask': '/background/geometry/#--crop-to-mask',
			'--fft-padding': '/background/geometry/#--fft-padding',
			'--ilsqr-pad-mm': '/background/geometry/#--ilsqr-pad-mm',
			'--ilsqr-precision': '/background/geometry/#--ilsqr-precision',
			'b0-directions-and-what-will-be-refused': '/reference/multi-orientation/#b0-directions-and-what-will-be-refused',
			'combining-sections': '/reference/masking/#combining-sections',
			'figures': '/reference/referencing-and-stats/#figures',
			'grid-padding-and-cropping': '/background/geometry/#grid-padding-and-cropping',
			'masking': '/reference/masking/#masking',
			'multi-orientation-cosmos-and-sti': '/reference/multi-orientation/#multi-orientation-cosmos-and-sti',
			'naming-a-region': '/reference/referencing-and-stats/#naming-a-region',
			'naming-the-orientation-group': '/reference/multi-orientation/#naming-the-orientation-group',
			'oblique-acquisitions': '/background/geometry/#oblique-acquisitions',
			'output-space': '/background/geometry/#output-space',
			'outputs': '/reference/multi-orientation/#outputs',
			'r2-coverage-and-field-strength': '/reference/source-separation/#r2-coverage-and-field-strength',
			'referencing': '/reference/referencing-and-stats/#referencing',
			'registration': '/reference/multi-orientation/#registration',
			'running-it': '/reference/multi-orientation/#running-it',
			'segmentation-and-per-structure-statistics': '/reference/referencing-and-stats/#segmentation-and-per-structure-statistics',
			'standalone': '/reference/multi-orientation/#standalone',
			'susceptibility-source-separation': '/reference/source-separation/#susceptibility-source-separation',
			'two-pass-modes': '/reference/masking/#two-pass-modes',
			'where-r2-comes-from': '/reference/source-separation/#where-r2-comes-from',
			'which-methods-need-axial-data': '/background/geometry/#which-methods-need-axial-data',
		},
	},
	'/guides/running-interactively/': {
		to: '/background/tui/',
		anchors: {
			'pipelines': '/toolbox-pipelines/#pipelines',
			'recommendations': '/background/recommendations/#recommendations',
			'updates-and-announcements': '/reference/updates/#updates-and-announcements',
		},
	},
};

// Pages of the restructured site that the how-to rewrite removed again. They were never on the
// released site, but the preview linked them.
Object.assign(moved, {
	'/how-to/prepare-data/': {
		to: '/data/',
		anchors: {
			'bringing-your-own-inputs': '/reference/inputs/#bringing-your-own-inputs',
			'ge-data': '/data/ge-real-imaginary/',
			'real-and-imaginary-images': '/data/ge-real-imaginary/',
			'slice-gaps-in-2d-acquisitions': '/data/2d-slice-gaps/',
			'uncombined-receive-coils': '/data/uncombined-coils/',
		},
	},
	'/how-to/run-bids/': { to: '/how-to/run-qsm/', anchors: {} },
	'/how-to/build-mask/': { to: '/reference/masking/', anchors: {} },
	'/how-to/multi-orientation/': { to: '/reference/multi-orientation/', anchors: {} },
	'/how-to/reference-and-stats/': { to: '/reference/referencing-and-stats/', anchors: {} },
	'/how-to/source-separation/': { to: '/reference/source-separation/', anchors: {} },
});

export const split = {
	'/how-to/slurm/': {
		anchors: {
			'parallelism-on-a-single-node': '/reference/slurm/#parallelism-on-a-single-node',
		},
	},
	'/how-to/convert-dicom/': {
		anchors: {
			'preview-first-with---dry-run': '/reference/dicom-conversion/#preview-first-with---dry-run',
		},
	},
	'/reference/inputs/': {
		anchors: {
			'ge-data': '/data/ge-real-imaginary/',
			'real-and-imaginary-images': '/data/ge-real-imaginary/',
			'slice-gaps-in-2d-acquisitions': '/data/2d-slice-gaps/',
			'uncombined-receive-coils': '/data/uncombined-coils/',
			'what-qsmxt-dicom-convert-writes': '/reference/dicom-conversion/#what-qsmxt-dicom-convert-writes',
		},
	},
	'/reference/configuration/': {
		anchors: {
			'generate-a-starting-config': '/how-to/run-qsm/#optional',
			'required-changes': '/background/recommendations/#required-changes',
			'use-it': '/how-to/run-qsm/#optional',
			'validate-before-you-run': '/how-to/run-qsm/#on-the-command-line',
		},
	},
};

export const v8 = [
	'/installation',
	'/data-acquisition',
	'/using-qsmxt',
	'/using-qsmxt/data-preparation',
	'/using-qsmxt/qsmxt',
	'/algorithms',
];
