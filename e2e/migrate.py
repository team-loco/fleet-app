import json
import sys

SELECTOR = {"flock.dev/scenario": "nonexistent-node"}


def is_stale(pod):
    return pod.get("spec", {}).get("nodeSelector") != SELECTOR


def main():
    sys.exit(0 if is_stale(json.load(sys.stdin)) else 1)


if __name__ == "__main__":
    main()
