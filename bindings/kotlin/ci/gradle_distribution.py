"""The shared, verified Gradle distribution for Android and desktop producers."""
import hashlib
import os
from pathlib import Path
import shutil
import urllib.request
import zipfile

GRADLE_VERSION = "8.13"
GRADLE_SHA256 = "20f1b1176237254a6fc204d8434196fa11a4cfb387567519c61556e8710aed78"


def gradle(directory: Path) -> Path:
    directory.mkdir(parents=True, exist_ok=True)
    archive = directory / "gradle.zip"
    with urllib.request.urlopen(f"https://services.gradle.org/distributions/gradle-{GRADLE_VERSION}-bin.zip", timeout=120) as response:
        with archive.open("wb") as stream:
            shutil.copyfileobj(response, stream)
    if hashlib.sha256(archive.read_bytes()).hexdigest() != GRADLE_SHA256:
        raise ValueError("Gradle distribution checksum mismatch")
    with zipfile.ZipFile(archive) as bundle:
        bundle.extractall(directory)
    archive.unlink()
    executable = directory / f"gradle-{GRADLE_VERSION}/bin/gradle"
    executable.chmod(0o755)
    return executable.with_suffix(".bat") if os.name == "nt" else executable
