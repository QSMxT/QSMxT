# ADAPTED FOR QSMXT BY ASHLEY STEWART (a.stewart.au@gmail.com)

'''
Copyright (c) 2009-2016, Nipype developers

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.

Prior to release 0.12, Nipype was licensed under a BSD license.
'''

import string
import re
import os
import os.path as op
import glob

from nipype.utils.misc import human_order_sorted
from nipype.utils.filemanip import simplify_list
from nipype.interfaces.io import SelectFiles, SelectFilesInputSpec, IOBase, add_traits
from nipype.interfaces.base import (
    TraitedSpec,
    traits,
    Str,
    File,
    Directory,
    BaseInterface,
    InputMultiPath,
    isdefined,
    OutputMultiPath,
    DynamicTraitedSpec,
    Undefined,
    BaseInterfaceInputSpec,
    LibraryBaseInterface,
    SimpleInterface,
)

class SelectFiles(IOBase):
    """
    Flexibly collect data from disk to feed into workflows.

    This interface uses Python's {}-based string formatting syntax to plug
    values (possibly known only at workflow execution time) into string
    templates and collect files from persistant storage. These templates can
    also be combined with glob wildcards (``*``, ``?``) and character ranges (``[...]``).
    The field names in the formatting template (i.e. the terms in braces) will
    become inputs fields on the interface, and the keys in the templates
    dictionary will form the output fields.

    Examples
    --------
    >>> import pprint
    >>> from nipype import SelectFiles, Node
    >>> templates={"T1": "{subject_id}/struct/T1.nii",
    ...            "epi": "{subject_id}/func/f[0,1].nii"}
    >>> dg = Node(SelectFiles(templates), "selectfiles")
    >>> dg.inputs.subject_id = "subj1"
    >>> pprint.pprint(dg.outputs.get())  # doctest:
    {'T1': <undefined>, 'epi': <undefined>}

    Note that SelectFiles does not support lists as inputs for the dynamic
    fields. Attempts to do so may lead to unexpected results because brackets
    also express glob character ranges. For example,

    >>> templates["epi"] = "{subject_id}/func/f{run}.nii"
    >>> dg = Node(SelectFiles(templates), "selectfiles")
    >>> dg.inputs.subject_id = "subj1"
    >>> dg.inputs.run = [10, 11]

    would match f0.nii or f1.nii, not f10.nii or f11.nii.

    """

    input_spec = SelectFilesInputSpec
    output_spec = DynamicTraitedSpec
    _always_run = True

    def __init__(self, templates, num_files=None, **kwargs):
        """Create an instance with specific input fields.

        Parameters
        ----------
        templates : dictionary
            Mapping from string keys to string template values.
            The keys become output fields on the interface.
            The templates should use {}-formatting syntax, where
            the names in curly braces become inputs fields on the interface.
            Format strings can also use glob wildcards to match multiple
            files. At runtime, the values of the interface inputs will be
            plugged into these templates, and the resulting strings will be
            used to select files.

        """
        super(SelectFiles, self).__init__(**kwargs)

        # Infer the infields and outfields from the template
        infields = []
        for name, template in list(templates.items()):
            for _, field_name, _, _ in string.Formatter().parse(template):
                if field_name is not None:
                    field_name = re.match("\w+", field_name).group()
                    if field_name not in infields:
                        infields.append(field_name)

        self._infields = infields
        self._outfields = list(templates)
        self._templates = templates
        self._num_files = num_files

        # Add the dynamic input fields
        undefined_traits = {}
        for field in infields:
            self.inputs.add_trait(field, traits.Any)
            undefined_traits[field] = Undefined
        self.inputs.trait_set(trait_change_notify=False, **undefined_traits)

    def _add_output_traits(self, base):
        """Add the dynamic output fields"""
        return add_traits(base, list(self._templates.keys()))

    def _list_outputs(self):
        """Find the files and expose them as interface outputs."""
        outputs = {}
        info = dict(
            [
                (k, v)
                for k, v in list(self.inputs.__dict__.items())
                if k in self._infields
            ]
        )

        force_lists = self.inputs.force_lists
        if isinstance(force_lists, bool):
            force_lists = self._outfields if force_lists else []
        bad_fields = set(force_lists) - set(self._outfields)
        if bad_fields:
            bad_fields = ", ".join(list(bad_fields))
            plural = "s" if len(bad_fields) > 1 else ""
            verb = "were" if len(bad_fields) > 1 else "was"
            msg = (
                "The field%s '%s' %s set in 'force_lists' and not in " "'templates'."
            ) % (plural, bad_fields, verb)
            raise ValueError(msg)

        for field, template in list(self._templates.items()):

            find_dirs = template[-1] == os.sep

            # Build the full template path
            if isdefined(self.inputs.base_directory):
                if self.inputs.base_directory == '/':
                    pass
                else:
                    template = op.abspath(op.join(self.inputs.base_directory, template))
            else:
                template = op.abspath(template)

            # re-add separator if searching exclusively for directories
            if find_dirs:
                template += os.sep

            # Fill in the template and glob for files
            filled_template = template.format(**info)
            filelist = glob.glob(filled_template)

            # Handle the case where nothing matched
            if not filelist:
                msg = "No files were found matching %s template: %s" % (
                    field,
                    filled_template,
                )
                if self.inputs.raise_on_empty:
                    raise IOError(msg)
                else:
                    warn(msg)

            # Possibly sort the list
            if self.inputs.sort_filelist:
                filelist = human_order_sorted(filelist)

            # Handle whether this must be a list or not
            if field not in force_lists:
                filelist = simplify_list(filelist)

            # Limit the number of files matched
            if self._num_files:
                filelist = filelist[:self._num_files]

            # Get number of files
            outputs['num_files'] = len(filelist)

            outputs[field] = filelist

        return outputs
