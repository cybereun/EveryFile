# -*- mode: python ; coding: utf-8 -*-
from pathlib import Path

root = Path(SPECPATH)

a = Analysis(
    ["ocr_host/main.py"],
    pathex=[str(root)],
    binaries=[],
    datas=[
        (str(root / "models"), "models"),
        (str(root / "models.json"), "."),
    ],
    hiddenimports=["paddleocr", "paddle", "paddlex"],
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
