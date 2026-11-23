// Old page URLs and where their content went (paths below the site's base, `/QSMxT`).
//
// `moved`: pages that no longer exist. Each becomes a redirect (`redirects` in
// astro.config.mjs) to `to`; a link to one of its headings goes to the page that heading moved to
// (`anchors`, old heading id → new page and id; `''` is a link with no heading). Headings not
// listed are on `to` under the same id.
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
			'trying-the-main-branch': '/how-to/update/#other-cases',
		},
	},
	'/getting-started/quick-start/': {
		to: '/start/quick-start/',
		anchors: {
			'1-convert-dicoms-to-bids': '/how-to/convert-dicom/',
			'2-run-the-pipeline': '/how-to/run-qsm/',
			'next-steps': '/start/quick-start/#next',
			'no-data-yet-fetch-an-example': '/start/quick-start/',
			'the-command-line-way': '/start/quick-start/',
			'the-recommended-way-the-tui': '/start/quick-start/',
			'what-you-get': '/how-to/results/',
		},
	},
	'/getting-started/coming-from-8x/': {
		to: '/start/install/',
		anchors: {
			// No heading in the link: the section, not the top of the page.
			'': '/start/install/#coming-from-qsmxt-8x',
		},
	},
	'/guides/running-noninteractively/': {
		to: '/how-to/run-qsm/',
		anchors: {
			'a-typical-cluster-workflow': '/how-to/slurm/#steps',
			'caching-plays-nicely-with-retries': '/how-to/slurm/#what-the-jobs-do',
			'choosing-algorithms': '/reference/methods/',
			'converting-dicoms-to-bids': '/how-to/convert-dicom/',
			'exporting-results-as-dicom': '/how-to/export-dicom/',
			'how-the-jobs-divide-the-work': '/how-to/slurm/#what-the-jobs-do',
			'hpc--slurm': '/how-to/slurm/',
			'keeping-the-studys-dicom-uids': '/how-to/convert-dicom/#keeping-the-studys-dicom-uids',
			'multi-echo-phase-handling': '/data/bipolar/',
			'output': '/reference/outputs/#the-methods-text-and-pipeline-graph',
			'parallelism-on-a-single-node': '/how-to/run-qsm/#optional',
			'pipelines': '/toolbox-pipelines/#pipelines-1',
			'preview-first-with---dry-run': '/how-to/convert-dicom/#steps',
			're-running': '/how-to/run-qsm/#optional',
			'recommendations': '/background/recommendations/#on-the-command-line',
			'running-the-pipeline': '/how-to/run-qsm/#on-the-command-line',
			'selecting-runs': '/how-to/run-qsm/#optional',
			'the-reference-summary': '/reference/outputs/#the-reference-summary',
			'the-stages': '/background/pipeline/#the-stages',
			'using-a-configuration-file': '/how-to/run-qsm/#optional',
			'what-it-handles': '/background/input-handling/#what-it-handles',
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
			'figures': '/reference/outputs/#figures',
			'grid-padding-and-cropping': '/background/geometry/#grid-padding-and-cropping',
			'masking': '/reference/masking/#masking',
			'multi-orientation-cosmos-and-sti': '/reference/multi-orientation/',
			'naming-a-region': '/background/pipeline/#naming-a-region',
			'naming-the-orientation-group': '/reference/multi-orientation/#naming-the-orientation-group',
			'oblique-acquisitions': '/background/geometry/#oblique-acquisitions',
			'output-space': '/background/geometry/#output-space',
			'outputs': '/reference/multi-orientation/#outputs',
			'r2-coverage-and-field-strength': '/reference/source-separation/#r2-coverage-and-field-strength',
			'referencing': '/background/pipeline/#referencing',
			'registration': '/reference/multi-orientation/#registration',
			'running-it': '/reference/multi-orientation/#running-it',
			'segmentation-and-per-structure-statistics': '/reference/outputs/#segmentation-and-per-structure-statistics',
			'standalone': '/reference/multi-orientation/#standalone',
			'susceptibility-source-separation': '/reference/source-separation/#methods',
			'two-pass-modes': '/reference/masking/#two-pass-modes',
			'where-r2-comes-from': '/reference/source-separation/#where-r2-comes-from',
			'which-methods-need-axial-data': '/background/geometry/#which-methods-need-axial-data',
		},
	},
	'/guides/running-interactively/': {
		to: '/background/tui/',
		anchors: {
			'pipelines': '/toolbox-pipelines/#pipelines',
			'recommendations': '/background/recommendations/#in-the-tui',
			'updates-and-announcements': '/how-to/update/#announcements',
			'what-you-can-do': '/start/quick-start/',
		},
	},
	'/reference/commands/': {
		to: '/reference/cli/',
		anchors: {
			'run-options-at-a-glance': '/reference/cli/#qsmxt-run',
		},
	},
	'/reference/tools/': {
		to: '/reference/cli/',
		anchors: {
			'background-field-removal--qsmxt-bgremove': '/reference/cli/#qsmxt-bgremove',
			'coil-combination--qsmxt-combine': '/reference/cli/#qsmxt-combine',
			'dipole-inversion--qsmxt-invert': '/reference/cli/#qsmxt-invert',
			'field-mapping--qsmxt-fieldmap': '/reference/cli/#qsmxt-fieldmap',
			'image-utilities': '/reference/cli/#qsmxt-homogeneity',
			'masking--qsmxt-mask': '/reference/cli/#qsmxt-mask',
			'phase-unwrapping--qsmxt-unwrap': '/reference/cli/#qsmxt-unwrap',
			'source-separation--qsmxt-separate': '/reference/cli/#qsmxt-separate',
			'supplementary-maps': '/reference/cli/#qsmxt-swi',
		},
	},
};

// Pages of the restructured site that were only ever on the preview, before it settled. Not
// on the released site, but the preview linked them.
Object.assign(moved, {
	'/start/tutorial-tui/': { to: '/start/quick-start/', anchors: {} },
	'/start/tutorial-cli/': { to: '/start/quick-start/', anchors: {} },
	'/start/coming-from-8x/': { to: '/start/install/', anchors: { '': '/start/install/#coming-from-qsmxt-8x' } },
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
	'/how-to/choose-methods/': { to: '/how-to/run-qsm/', anchors: {} },
	'/how-to/build-mask/': { to: '/reference/masking/', anchors: {} },
	'/how-to/multi-orientation/': { to: '/reference/multi-orientation/', anchors: {} },
	'/how-to/reference-and-stats/': { to: '/background/pipeline/', anchors: {
		'segmentation-and-per-structure-statistics': '/reference/outputs/#segmentation-and-per-structure-statistics',
		'figures': '/reference/outputs/#figures',
	} },
	'/how-to/source-separation/': { to: '/reference/source-separation/', anchors: {} },
	'/how-to/chi-separation/': { to: '/data/source-separation/', anchors: {} },
	'/how-to/structure-stats/': { to: '/how-to/results/', anchors: { '': '/how-to/results/#measure-χ-in-brain-structures' } },
	'/data/mese-r2prime/': { to: '/data/source-separation/', anchors: {} },
	'/reference/dicom-conversion/': { to: '/how-to/convert-dicom/', anchors: {
		'what-it-handles': '/background/input-handling/#what-it-handles',
		'what-qsmxt-dicom-convert-writes': '/reference/inputs/#what-qsmxt-dicom-convert-writes',
	} },
	'/reference/dicom-export/': { to: '/reference/outputs/', anchors: { '': '/reference/outputs/#dicom-series' } },
	'/reference/slurm/': { to: '/how-to/slurm/', anchors: {} },
	'/reference/updates/': { to: '/how-to/update/', anchors: {} },
	'/reference/referencing-and-stats/': { to: '/background/pipeline/', anchors: {
		'segmentation-and-per-structure-statistics': '/reference/outputs/#segmentation-and-per-structure-statistics',
		'figures': '/reference/outputs/#figures',
	} },
});

// Pages that still exist but gave some sections to other pages; a link to one of those sections
// is sent on to its new page.
export const split = {
	'/reference/source-separation/': {
		anchors: {
			'susceptibility-source-separation': '/reference/source-separation/#methods',
		},
	},
	'/background/recommendations/': {
		anchors: {
			'recommendations': '/background/recommendations/#in-the-tui',
			'recommendations-1': '/background/recommendations/#on-the-command-line',
		},
	},
	'/reference/inputs/': {
		anchors: {
			'ge-data': '/data/ge-real-imaginary/',
			'real-and-imaginary-images': '/data/ge-real-imaginary/',
			'slice-gaps-in-2d-acquisitions': '/data/2d-slice-gaps/',
			'uncombined-receive-coils': '/data/uncombined-coils/',
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
