from __future__ import annotations

import ipaddress
import json
import os
from pathlib import Path
import socket
import sys
from threading import Lock
from typing import Any, Iterable

SUPPORTED_EXTENSIONS = {".pdf", ".jpg", ".jpeg", ".png", ".webp", ".bmp", ".tif", ".tiff"}
MAX_RESPONSE_TEXT = 20_000_000


class ProtocolError(Exception):
    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code


def resource_root() -> Path:
    bundled = getattr(sys, "_MEIPASS", None)
    return Path(bundled) if bundled else Path(__file__).resolve().parents[1]


def install_network_guard() -> None:
    original_connect = socket.socket.connect
    original_create_connection = socket.create_connection

    def is_loopback(address: Any) -> bool:
        host = address[0] if isinstance(address, tuple) and address else address
        if not isinstance(host, str):
            return False
        if host.lower() == "localhost":
            return True
        try:
            return ipaddress.ip_address(host).is_loopback
        except ValueError:
            return False

    def guarded_connect(sock: socket.socket, address: Any) -> Any:
        if not is_loopback(address):
            raise OSError("EveryFile OCR blocks external network access")
        return original_connect(sock, address)

    def guarded_create_connection(address: Any, *args: Any, **kwargs: Any) -> socket.socket:
        if not is_loopback(address):
            raise OSError("EveryFile OCR blocks external network access")
        return original_create_connection(address, *args, **kwargs)

    socket.socket.connect = guarded_connect  # type: ignore[method-assign]
    socket.create_connection = guarded_create_connection  # type: ignore[assignment]


def json_value(result: Any) -> dict[str, Any]:
    value = getattr(result, "json", result)
    if callable(value):
        value = value()
    if not isinstance(value, dict):
        raise ProtocolError("INVALID_ENGINE_RESULT", "OCR engine returned an invalid result")
    if isinstance(value.get("res"), dict):
        value = value["res"]
    return value


def normalise_box(value: Any) -> list[list[float]] | None:
    if hasattr(value, "tolist"):
        value = value.tolist()
    if not isinstance(value, list):
        return None
    return value


def text_document(results: Iterable[Any]) -> dict[str, Any]:
    pages: list[dict[str, Any]] = []
    all_text: list[str] = []
    for page_number, result in enumerate(results, start=1):
        value = json_value(result)
        texts = [str(item) for item in value.get("rec_texts", []) if str(item).strip()]
        scores = value.get("rec_scores", [])
        boxes = value.get("rec_polys", [])
        lines = []
        for index, text in enumerate(texts):
            score = float(scores[index]) if index < len(scores) else None
            box = normalise_box(boxes[index]) if index < len(boxes) else None
            lines.append({"text": text, "score": score, "box": box})
        raw_page = value.get("page_index")
        page = (raw_page if isinstance(raw_page, int) else page_number - 1) + 1
        pages.append({"page": page, "lines": lines})
        all_text.extend(texts)
    plain_text = "\n".join(all_text)[:MAX_RESPONSE_TEXT]
    return {
        "plainText": plain_text,
        "markdown": plain_text,
        "blocks": pages,
        "metadata": {"engine": "PaddleOCR", "localOnly": True, "pageCount": len(pages)},
        "warnings": [],
    }


def merge_formulae(document: dict[str, Any], results: Iterable[Any]) -> dict[str, Any]:
    formulae: list[dict[str, Any]] = []
    for result in results:
        value = json_value(result)
        for item in value.get("formula_res_list", []):
            if not isinstance(item, dict):
                continue
            latex = str(item.get("rec_formula", "")).strip()
            if latex:
                formulae.append(
                    {
                        "latex": latex,
                        "region": item.get("dt_polys"),
                        "page": (
                            value["page_index"]
                            if isinstance(value.get("page_index"), int)
                            else 0
                        )
                        + 1,
                    }
                )
    if formulae:
        math_markdown = "\n\n".join(f"$$\n{item['latex']}\n$$" for item in formulae)
        document["markdown"] = f"{document['markdown']}\n\n{math_markdown}".strip()
        document["plainText"] = (
            f"{document['plainText']}\n" + "\n".join(item["latex"] for item in formulae)
        ).strip()[:MAX_RESPONSE_TEXT]
        document["blocks"].append({"type": "formulae", "items": formulae})
    document["metadata"]["mathOcr"] = True
    return document


class PaddleEngine:
    def __init__(self, root: Path | None = None) -> None:
        self.root = root or resource_root()
        self.models = self.root / "models"
        self._text: Any = None
        self._formula: Any = None
        self._lock = Lock()

    def _require_model(self, name: str) -> Path:
        path = self.models / name
        if not path.is_dir():
            raise ProtocolError("MODEL_MISSING", f"Required local OCR model is missing: {name}")
        return path

    def text_pipeline(self) -> Any:
        with self._lock:
            if self._text is None:
                install_network_guard()
                from paddleocr import PaddleOCR

                self._text = PaddleOCR(
                    text_detection_model_name="PP-OCRv5_mobile_det",
                    text_detection_model_dir=str(self._require_model("PP-OCRv5_mobile_det")),
                    text_recognition_model_name="korean_PP-OCRv5_mobile_rec",
                    text_recognition_model_dir=str(
                        self._require_model("korean_PP-OCRv5_mobile_rec")
                    ),
                    use_doc_orientation_classify=False,
                    use_doc_unwarping=False,
                    use_textline_orientation=False,
                    lang="korean",
                    ocr_version="PP-OCRv5",
                    device="cpu",
                    cpu_threads=max(1, min(4, os.cpu_count() or 1)),
                    enable_mkldnn=False,
                )
            return self._text

    def formula_pipeline(self) -> Any:
        with self._lock:
            if self._formula is None:
                install_network_guard()
                from paddleocr import FormulaRecognitionPipeline

                self._formula = FormulaRecognitionPipeline(
                    layout_detection_model_name="PP-DocLayout-S",
                    layout_detection_model_dir=str(self._require_model("PP-DocLayout-S")),
                    formula_recognition_model_name="PP-FormulaNet-S",
                    formula_recognition_model_dir=str(
                        self._require_model("PP-FormulaNet-S")
                    ),
                    use_doc_orientation_classify=False,
                    use_doc_unwarping=False,
                    use_layout_detection=True,
                    device="cpu",
                    cpu_threads=max(1, min(4, os.cpu_count() or 1)),
                    enable_mkldnn=False,
                )
            return self._formula

    def recognise(self, path: Path, include_math: bool) -> dict[str, Any]:
        document = text_document(
            self.text_pipeline().predict(
                str(path),
                use_doc_orientation_classify=False,
                use_doc_unwarping=False,
                use_textline_orientation=False,
            )
        )
        if include_math:
            document = merge_formulae(
                document,
                self.formula_pipeline().predict(
                    str(path),
                    use_layout_detection=True,
                    use_doc_orientation_classify=False,
                    use_doc_unwarping=False,
                ),
            )
        return document


def handle_request(request: Any, engine: Any) -> dict[str, Any]:
    request_id = request.get("id") if isinstance(request, dict) else None
    try:
        if not isinstance(request, dict) or request.get("operation") != "ocr":
            raise ProtocolError("INVALID_REQUEST", "Invalid OCR request")
        if not isinstance(request_id, str) or not request_id:
            raise ProtocolError("INVALID_REQUEST", "Request id is required")
        raw_path = request.get("path")
        mode = request.get("mode")
        max_bytes = request.get("maxBytes")
        if not isinstance(raw_path, str) or mode not in {"text", "math"}:
            raise ProtocolError("INVALID_REQUEST", "Path and OCR mode are required")
        if not isinstance(max_bytes, int) or max_bytes <= 0 or max_bytes > 500_000_000:
            raise ProtocolError("INVALID_REQUEST", "Invalid OCR size limit")
        path = Path(raw_path)
        if not path.is_file():
            raise ProtocolError("FILE_UNAVAILABLE", "OCR source file is unavailable")
        if path.suffix.lower() not in SUPPORTED_EXTENSIONS:
            raise ProtocolError("UNSUPPORTED", "OCR source format is unsupported")
        if path.stat().st_size > max_bytes:
            raise ProtocolError("TOO_LARGE", "OCR source exceeds the configured size limit")
        document = engine.recognise(path.resolve(), mode == "math")
        return {"id": request_id, "ok": True, "document": document}
    except ProtocolError as error:
        return {
            "id": request_id if isinstance(request_id, str) else None,
            "ok": False,
            "error": {"code": error.code, "message": str(error)},
        }
    except Exception:
        if os.environ.get("EVERYFILE_OCR_DIAGNOSTICS") == "1":
            import traceback

            traceback.print_exc(file=sys.stderr)
        return {
            "id": request_id if isinstance(request_id, str) else None,
            "ok": False,
            "error": {"code": "INTERNAL", "message": "Local OCR failed"},
        }


def _write_response(response: dict[str, object]) -> bool:
    try:
        sys.stdout.write(json.dumps(response, ensure_ascii=False, separators=(",", ":")) + "\n")
        sys.stdout.flush()
        return True
    except (BrokenPipeError, OSError, ValueError):
        # The desktop parent owns this pipe. Closing the app while OCR is finishing
        # is a normal shutdown path and must never surface a PyInstaller error box.
        return False


def run() -> None:
    engine = PaddleEngine()
    try:
        for line in sys.stdin:
            try:
                request = json.loads(line)
            except json.JSONDecodeError:
                response = {
                    "id": None,
                    "ok": False,
                    "error": {"code": "INVALID_REQUEST", "message": "Invalid JSON"},
                }
            else:
                response = handle_request(request, engine)
            if not _write_response(response):
                return
    except (OSError, ValueError):
        # stdin can become invalid when the GUI parent exits or cancels indexing.
        return


if __name__ == "__main__":
    try:
        run()
    except Exception:
        # A windowed PyInstaller executable displays uncaught exceptions in a
        # modal dialog. Exit quietly; the Rust parent reports a bounded OCR error.
        raise SystemExit(1) from None
