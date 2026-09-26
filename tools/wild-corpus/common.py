"""Shared settings for the wild-corpus scripts.

Collected data goes to a work directory ($FMTRON_WILD_WORK, default
./wild-work relative to the current directory), never into the repository;
only build_corpus.py writes into the repository's test_data/wild/.
"""
import os

TOOLS = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(TOOLS))
WORK = os.path.abspath(os.environ.get("FMTRON_WILD_WORK", "wild-work"))
os.makedirs(WORK, exist_ok=True)
# APIs such as crates.io ask for an identifying User-Agent.
UA = "fmtron-corpus-collector (https://github.com/barafael/fmtron)"
