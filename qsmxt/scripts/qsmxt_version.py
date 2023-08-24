#!/usr/bin/env python3
import argparse
from qsmxt.scripts.qsmxt_functions import get_qsmxt_version, get_container_version

if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="QSMxT: Version checker",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter
    )

    parser.add_argument(
        '--container_only',
        action='store_true'
    )
    
    args = parser.parse_args()
    
    if args.container_only:
        print(get_container_version(check_path=False))
    else:
        print(f"{get_qsmxt_version()} (container version {get_container_version(check_path=False)})")

    