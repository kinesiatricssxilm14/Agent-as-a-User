"""toolh - a keyboard-driven TUI for organizing, searching and reusing code snippets.

The package is deliberately split so that every layer can be used (and tested)
without a terminal attached:

``toolh.config``     resolution of the on-disk storage location
``toolh.models``     the :class:`~toolh.models.Snippet` record
``toolh.storage``    real SQLite persistence (CRUD + search)
``toolh.clipboard``  real clipboard integration with layered backends
``toolh.app``        the Textual user interface
``toolh.cli``        the ``toolh`` console entry point
"""

__all__ = ["__version__"]

__version__ = "1.0.0"
