import subprocess


def sys_cmd(cmd, print_output=True, print_command=True):
    if print_command:
        print(cmd)
    
    result_byte = subprocess.run(cmd, shell=True, stdout=subprocess.PIPE).stdout
    results     = result_byte.decode('UTF-8')[:-2]
    
    if print_output:
        print(results, end="")
    
    return results

