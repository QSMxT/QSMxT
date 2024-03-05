import subprocess


def sys_cmd(cmd, print_output=True, print_command=True, raise_exception=False):
    if print_command:
        print(cmd)

    process = subprocess.run(cmd, shell=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    results = process.stdout.decode('UTF-8')[:-1] + '\n' + process.stderr.decode('UTF-8')[:-1]

    if print_output:
        print(results, end="")

    if raise_exception and process.returncode != 0:
        raise subprocess.CalledProcessError(process.returncode, cmd, output=process.stderr.decode('UTF-8')[:-1])

    return results

