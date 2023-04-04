# VM:
#   status: success
#   rerun-if-status: 42

import os, sys

cookie = os.path.join(os.environ["CARGO_TARGET_TMPDIR"], "rerun_status_cookie")
i = 0
if os.path.exists(cookie):
    i = int(open(cookie, "r").read().strip()) + 1
    if i == 5:
        sys.exit(0)

open(cookie, "w").write(str(i))
sys.exit(42)
