#!/usr/bin/env python3
"""Export every macOS 27 appearance of icon_file/SISSY_ICON_MAC.icon.

Icon Composer keeps the colored mascot on the normal and dark plates, and the
mono mascot on the tinted plates. ictool renders each plate with the system
squircle. The app picks one at runtime from the Appearance setting.
"""
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ICON = ROOT / "icon_file" / "SISSY_ICON_MAC.icon"
ASSETS = ROOT / "assets"
ICTOOL = Path("/Applications/Icon Composer.app/Contents/Executables/ictool")

RENDITIONS = {
    "Default": "icon-default.png",
    "Dark": "icon-dark.png",
    "ClearLight": "icon-clear-light.png",
    "ClearDark": "icon-clear-dark.png",
    "TintedLight": "icon-tinted-light.png",
    "TintedDark": "icon-tinted-dark.png",
}


def export(rendition: str, dest: Path) -> None:
    if not ICTOOL.is_file():
        sys.exit(f"Icon Composer não encontrado em {ICTOOL}")
    subprocess.run(
        [
            str(ICTOOL),
            str(ICON),
            "--export-image",
            "--output-file",
            str(dest),
            "--platform",
            "macOS",
            "--rendition",
            rendition,
            "--width",
            "1024",
            "--height",
            "1024",
            "--scale",
            "1",
            "--design-generation",
            "27",
        ],
        check=True,
    )


def main() -> None:
    ASSETS.mkdir(parents=True, exist_ok=True)
    for rendition, name in RENDITIONS.items():
        export(rendition, ASSETS / name)
        print(ASSETS / name)
    shutil.copyfile(ASSETS / "icon-default.png", ASSETS / "Sissy-composed.png")


if __name__ == "__main__":
    main()
