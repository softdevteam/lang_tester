# VM:
#   status: success
#   stdout:
#     $1
#     b
#     $1

import random

ALPHABET = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m",
            "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "y", "z"]

x = random.choice(ALPHABET)
print(x)
print("b")
print(x)
