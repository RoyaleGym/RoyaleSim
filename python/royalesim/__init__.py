"""RoyaleSim: a deterministic, integer-only Clash Royale battle engine.

The engine is the compiled ``royalesim.royalesim`` module; everything in it is re-exported here.
"""

from pathlib import Path

from . import royalesim as _native
from .royalesim import *  # noqa: F403

__doc__ = _native.__doc__
__all__ = [*(n for n in dir(_native) if not n.startswith("_")), "data_dir"]


def data_dir() -> Path:
    """The folder holding the engine's data files (calibration.json, derived/, raw/).

    In a checkout built with ``maturin develop`` this is the checkout's own ``data/``. In an installed wheel it is
    the copy shipped inside the package. The engine reads its card table from the same place: the checkout's
    ``derived/cards.json`` when it exists, else the copy compiled into the extension, which is the file shipped here.
    """
    built = Path(_native.BUILD_DATA_DIR)
    if (built / "derived" / "cards.json").is_file():
        return built.resolve()
    return Path(__file__).resolve().parent / "data"
