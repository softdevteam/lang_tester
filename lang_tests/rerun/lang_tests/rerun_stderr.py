# VM:
#   status: success
#   stderr: a
#   rerun-if-stderr: b

import os, sys

cookie = os.path.join(os.environ["CARGO_TARGET_TMPDIR"], "rerun_stderr_cookie")
i = 0
if os.path.exists(cookie):
    i = int(open(cookie, "r").read().strip())
    if i == 5:
        sys.stderr.write("a")
        sys.exit(0)
    i += 1

open(cookie, "w").write(str(i))
sys.stderr.write("b")
