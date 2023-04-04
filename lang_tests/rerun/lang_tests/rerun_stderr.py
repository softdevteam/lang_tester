# VM:
#   status: success
#   stderr: a
#   rerun-if-stderr: b

import os, sys

cookie = os.path.join(os.environ["CARGO_TARGET_TMPDIR"], "rerun_stderr_cookie")
i = 0
if os.path.exists(cookie):
    i = int(open(cookie, "r").read().strip()) + 1
    if i == 5:
        sys.stderr.write("a")
        sys.exit(0)

open(cookie, "w").write(str(i))
sys.stderr.write("b")
