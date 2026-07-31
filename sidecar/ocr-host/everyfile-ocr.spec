# -*- mode: python ; coding: utf-8 -*-
from pathlib import Path
from PyInstaller.utils.hooks import (
    collect_data_files,
    collect_dynamic_libs,
    collect_submodules,
    copy_metadata,
)

root = Path(SPECPATH)
package_data = collect_data_files("paddlex", includes=["configs/**/*.yaml"])
metadata_packages = [
    "paddlex",
    "paddleocr",
    "beautifulsoup4",
    "einops",
    "ftfy",
    "imagesize",
    "Jinja2",
    "latex2mathml",
    "lxml",
    "opencv-contrib-python",
    "openpyxl",
    "premailer",
    "pyclipper",
    "pypdfium2",
    "python-bidi",
    "regex",
    "safetensors",
    "scikit-learn",
    "scipy",
    "sentencepiece",
    "shapely",
    "tiktoken",
    "tokenizers",
]
package_metadata = [
    item
    for package in metadata_packages
    for item in copy_metadata(package)
]
dynamic_imports = (
    collect_submodules("scipy._external.array_api_compat")
    + collect_submodules("sklearn.externals.array_api_compat")
)
paddle_binaries = collect_dynamic_libs("paddle", destdir="paddle/libs")

a = Analysis(
    ["ocr_host/main.py"],
    pathex=[str(root)],
    binaries=paddle_binaries,
    datas=[
        (str(root / "models"), "models"),
        (str(root / "models.json"), "."),
    ] + package_data + package_metadata,
    hiddenimports=["paddleocr", "paddle", "paddlex"] + dynamic_imports,
)
pyz = PYZ(a.pure)
exe = EXE(
    pyz,
    a.scripts,
    a.binaries,
    a.datas,
    [],
    name="everyfile-ocr",
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=False,
    console=False,
)
