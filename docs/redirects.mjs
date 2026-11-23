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
			'keeping-up-to-date': '/how-to/update/#keeping-up-to-date',
			'trying-the-main-branch': '/how-to/update/#trying-the-main-branch',
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
		to: '/how-to/run-bids/',
		anchors: {
			'a-typical-cluster-workflow': '/how-to/slurm/#a-typical-cluster-workflow',
			'caching-plays-nicely-with-retries': '/how-to/slurm/#caching-plays-nicely-with-retries',
			'choosing-algorithms': '/how-to/choose-methods/#choosing-algorithms',
			'converting-dicoms-to-bids': '/how-to/convert-dicom/#converting-dicoms-to-bids',
			'exporting-results-as-dicom': '/how-to/export-dicom/#exporting-results-as-dicom',
			'how-the-jobs-divide-the-work': '/how-to/slurm/#how-the-jobs-divide-the-work',
			'hpc--slurm': '/how-to/slurm/#hpc--slurm',
			'keeping-the-studys-dicom-uids': '/how-to/convert-dicom/#keeping-the-studys-dicom-uids',
			'output': '/reference/outputs/#output',
			'parallelism-on-a-single-node': '/how-to/slurm/#parallelism-on-a-single-node',
			'pipelines': '/toolbox-pipelines/#pipelines-1',
			'preview-first-with---dry-run': '/how-to/convert-dicom/#preview-first-with---dry-run',
			'recommendations': '/background/recommendations/#recommendations-1',
			'the-reference-summary': '/reference/outputs/#the-reference-summary',
			'the-stages': '/background/pipeline/#the-stages',
			'what-it-handles': '/how-to/convert-dicom/#what-it-handles',
		},
	},
	'/reference/algorithms/': {
		to: '/reference/methods/',
		anchors: {
			'--crop-to-mask': '/background/geometry/#--crop-to-mask',
			'--fft-padding': '/background/geometry/#--fft-padding',
			'--ilsqr-pad-mm': '/background/geometry/#--ilsqr-pad-mm',
			'--ilsqr-precision': '/background/geometry/#--ilsqr-precision',
			'b0-directions-and-what-will-be-refused': '/how-to/multi-orientation/#b0-directions-and-what-will-be-refused',
			'combining-sections': '/how-to/build-mask/#combining-sections',
			'figures': '/how-to/reference-and-stats/#figures',
			'grid-padding-and-cropping': '/background/geometry/#grid-padding-and-cropping',
			'masking': '/how-to/build-mask/#masking',
			'multi-orientation-cosmos-and-sti': '/how-to/multi-orientation/#multi-orientation-cosmos-and-sti',
			'naming-a-region': '/how-to/reference-and-stats/#naming-a-region',
			'naming-the-orientation-group': '/how-to/multi-orientation/#naming-the-orientation-group',
			'oblique-acquisitions': '/background/geometry/#oblique-acquisitions',
			'output-space': '/background/geometry/#output-space',
			'outputs': '/how-to/multi-orientation/#outputs',
			'r2-coverage-and-field-strength': '/how-to/source-separation/#r2-coverage-and-field-strength',
			'referencing': '/how-to/reference-and-stats/#referencing',
			'registration': '/how-to/multi-orientation/#registration',
			'running-it': '/how-to/multi-orientation/#running-it',
			'segmentation-and-per-structure-statistics': '/how-to/reference-and-stats/#segmentation-and-per-structure-statistics',
			'standalone': '/how-to/multi-orientation/#standalone',
			'susceptibility-source-separation': '/how-to/source-separation/#susceptibility-source-separation',
			'two-pass-modes': '/how-to/build-mask/#two-pass-modes',
			'where-r2-comes-from': '/how-to/source-separation/#where-r2-comes-from',
			'which-methods-need-axial-data': '/background/geometry/#which-methods-need-axial-data',
		},
	},
	'/guides/running-interactively/': {
		to: '/background/tui/',
		anchors: {
			'pipelines': '/toolbox-pipelines/#pipelines',
			'recommendations': '/background/recommendations/#recommendations',
			'updates-and-announcements': '/how-to/update/#updates-and-announcements',
		},
	},
};

export const split = {
	'/reference/inputs/': {
		anchors: {
			'bringing-your-own-inputs': '/how-to/prepare-data/#bringing-your-own-inputs',
			'ge-data': '/how-to/prepare-data/#ge-data',
			'real-and-imaginary-images': '/how-to/prepare-data/#real-and-imaginary-images',
			'slice-gaps-in-2d-acquisitions': '/how-to/prepare-data/#slice-gaps-in-2d-acquisitions',
			'uncombined-receive-coils': '/how-to/prepare-data/#uncombined-receive-coils',
			'what-qsmxt-dicom-convert-writes': '/how-to/convert-dicom/#what-qsmxt-dicom-convert-writes',
		},
	},
	'/reference/configuration/': {
		anchors: {
			'generate-a-starting-config': '/how-to/run-bids/#generate-a-starting-config',
			'required-changes': '/background/recommendations/#required-changes',
			'use-it': '/how-to/run-bids/#use-it',
			'validate-before-you-run': '/how-to/run-bids/#validate-before-you-run',
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
