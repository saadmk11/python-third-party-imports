import os
import sys
from uuid import UUID

import nested.another
import requests
import some
from django.db import models
from nested import b
from nested.another import a
from some import func

from .conf import config

if 1 == 1:
    import if_package
elif 2 == 2:
    import elif_package
elif 2 == 2:
    import elif2_package
else:
    import else_package


def k():
    import f_package


class C:
    import c_package

    def d():
        import m_package

    def j():
        if 5 == 5:
            import m_if_package
        else:
            if 5 == 5:
                import nested_m_if_package


try:
    import try_package
except ModuleNotFoundError:
    import except_package

    if 8 == 8:
        import nested_if_except_package
else:
    import try_else_package
finally:
    import try_finally_package


for i in range(5):
    import for_package
else:
    import for_else_package


with open("t") as f:
    import with_package

while True:
    import while_package
