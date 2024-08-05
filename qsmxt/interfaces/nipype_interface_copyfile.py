from nipype.interfaces.base import BaseInterface, BaseInterfaceInputSpec, TraitedSpec, File, Directory, traits, DynamicTraitedSpec, Undefined, isdefined
import shutil
import os

class DynamicCopyFilesInputSpec(DynamicTraitedSpec, BaseInterfaceInputSpec):
    output_map = traits.Dict(mandatory=True, desc="Dictionary mapping inputs to their target output paths")
    parameterization = traits.Bool(True, usedefault=True, desc="Store output in parametrized structure")
    remove_dest_dir = traits.Bool(False, usedefault=True, desc="Remove destination directory when copying dirs")

class DynamicCopyFilesOutputSpec(TraitedSpec):
    out_files = traits.List(File(exists=True), desc="List of copied output files")
    out_dirs = traits.List(Directory(exists=True), desc="List of copied output directories")

class DynamicCopyFiles(BaseInterface):
    input_spec = DynamicCopyFilesInputSpec
    output_spec = DynamicCopyFilesOutputSpec

    def __init__(self, infields=None, **kwargs):
        super().__init__(**kwargs)
        self._results = {}
        if infields:
            undefined_traits = {}
            for key in infields:
                self.inputs.add_trait(key, traits.Any)
                undefined_traits[key] = Undefined
            self.inputs.trait_set(trait_change_notify=False, **undefined_traits)

    def _run_interface(self, runtime):
        output_map = self.inputs.output_map
        copied_files = []
        copied_dirs = []

        for name in self.inputs.copyable_trait_names():
            if name in ['output_map', 'parameterization', 'remove_dest_dir']:
                continue

            value = getattr(self.inputs, name)
            if not isdefined(value):
                continue
            
            if isinstance(value, str):
                value = [value]

            if isinstance(value, list) and len(value) == 1:
                value = value[0]

            if isinstance(value, str):
                if name in output_map:
                    dst = output_map[name]
                else:
                    dst = self._substitute_path(name, value)

                # Ensure the target directory exists
                os.makedirs(os.path.dirname(dst), exist_ok=True)

                if os.path.exists(value):
                    if os.path.isfile(value):
                        file_extension = '.' + '.'.join(os.path.split(value)[1].split('.')[1:])
                        dst_with_extension = f"{dst}{file_extension}"
                        shutil.copy2(value, dst_with_extension)
                        copied_files.append(dst_with_extension)
                    elif os.path.isdir(value):
                        if os.path.exists(dst) and self.inputs.remove_dest_dir:
                            shutil.rmtree(dst)
                        shutil.copytree(value, dst)
                        copied_dirs.append(dst)
            elif isinstance(value, list):
                dst_dir = output_map[name] if name in output_map else os.path.join(output_map['base_directory'], name)
                os.makedirs(dst_dir, exist_ok=True)
                for src in value:
                    if os.path.isfile(src):
                        file_dst = os.path.join(dst_dir, os.path.basename(src))
                        shutil.copy2(src, file_dst)
                        copied_files.append(file_dst)
                    elif os.path.isdir(src):
                        dir_dst = os.path.join(dst_dir, os.path.basename(src))
                        if os.path.exists(dir_dst) and self.inputs.remove_dest_dir:
                            shutil.rmtree(dir_dst)
                        shutil.copytree(src, dir_dst)
                        copied_dirs.append(dir_dst)

        self._results['out_files'] = copied_files
        self._results['out_dirs'] = copied_dirs

        return runtime

    def _list_outputs(self):
        outputs = self._results
        return outputs

    def _substitute_path(self, name, path):
        return os.path.basename(path)  # Adjust this if you have specific path substitution logic
    
    