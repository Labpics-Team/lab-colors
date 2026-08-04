"""Independent third-verifier semantic boundary for region proof V1.

The package replays every transcript decision from immutable job bytes using
only the canonical protocol module and the standard library.  It never imports
Arb or MPFI code and never compares engine transcripts against each other.
"""

from .receipt import (
    SemanticVerificationReasonV1,
    SemanticVerificationReceiptV1,
    SemanticVerificationRejectedV1,
)
from .verifier import verify_transcript

__all__ = [
    "SemanticVerificationReasonV1",
    "SemanticVerificationReceiptV1",
    "SemanticVerificationRejectedV1",
    "verify_transcript",
]
