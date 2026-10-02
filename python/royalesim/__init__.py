"""RoyaleSim: a deterministic, integer-only Clash Royale battle engine.

The engine is the compiled ``royalesim.royalesim`` module; everything in it is re-exported here.
"""

from importlib.metadata import PackageNotFoundError, version
from pathlib import Path

from . import royalesim as _native
from .royalesim import *  # noqa: F403

__doc__ = _native.__doc__
try:
    __version__ = version("royalesim")
except PackageNotFoundError:  # imported from a source tree that was never installed
    __version__ = "0+unknown"
__all__ = [*(n for n in dir(_native) if not n.startswith("_")), "data_dir", "__version__"]


def data_dir() -> Path:
    """The folder holding the engine's data files (calibration.json, derived/, raw/).

    In a checkout built with ``maturin develop`` this is the checkout's own ``data/``. In an installed wheel it is
    always the copy shipped inside the package: a wheel never looks at the machine it was built on. The engine reads
    its card table the same way: a checkout build reads the checkout's ``derived/cards.json`` when it exists; a wheel
    runs the copy compiled into it, which is the ``derived/cards.json`` shipped here.
    """
    built = getattr(_native, "BUILD_DATA_DIR", None)
    if built is not None and (Path(built) / "derived" / "cards.json").is_file():
        return Path(built).resolve()
    return Path(__file__).resolve().parent / "data"
