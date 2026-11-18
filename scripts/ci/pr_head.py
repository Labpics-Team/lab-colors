"""Stop only this PR check when GitHub proves that its head has advanced."""

import json
import os
import re
import subprocess
import sys


def require_current(env):
    repo = env.get("GITHUB_REPOSITORY", "")
    number, sha = env.get("PR_NUMBER", ""), env.get("PR_HEAD", "")
    if (env.get("GITHUB_API_URL") != "https://api.github.com" or env.get("PR_REPOSITORY") != repo
            or not re.fullmatch(r"[a-zA-Z0-9_.-]+/[a-zA-Z0-9_.-]+", repo)
            or not re.fullmatch(r"[1-9][0-9]*", number) or not re.fullmatch(r"[0-9a-f]{40}", sha)):
        return
    base = f"https://api.github.com/repos/{repo}/compare/{sha}..."
    reference = f"refs/pull/{number}/head"
    try:
        token = env.get("LABCOLORS_PR_TOKEN", "")
        header = f"Authorization: Bearer {token}\n".encode() if token else b""
        response = subprocess.run(["curl", "--disable", "--proto", "=https", "--fail", "--silent", "--max-time", "4", "-H", "@-",
            base + reference.replace("/", "%2F") + "?per_page=1&page=2"],
            input=header, capture_output=True, check=True, timeout=5)
        record = json.loads(response.stdout)
        if (record["url"] != base + reference or record["base_commit"]["sha"] != sha
                or record["merge_base_commit"]["sha"] != sha or type(record["behind_by"]) is not int
                or record["behind_by"] != 0):
            return
        ahead, status = record["ahead_by"], record["status"]
    except (OSError, subprocess.SubprocessError, ValueError, KeyError, TypeError):
        return
    if type(ahead) is int and 0 < ahead <= (1 << 53) - 1 and status == "ahead":
        print(f"PR {number} has advanced past {sha}. Ending this attempt.", file=sys.stderr)
        raise SystemExit(1)
    if type(ahead) is int and ahead == 0 and status == "identical":
        print(f"PR {number} still points to {sha}.", flush=True)


if __name__ == "__main__":
    require_current(os.environ)
