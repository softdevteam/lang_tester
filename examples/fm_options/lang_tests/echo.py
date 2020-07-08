# VM:
#   status: success
#   stdin:
#     a
#       b
#     a
#   stdout:
#     $1
#       b
#     $1

import sys

for l in sys.stdin:
    sys.stdout.write(l)
