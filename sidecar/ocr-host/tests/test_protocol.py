import tempfile
from pathlib import Path
import unittest

from ocr_host.main import handle_request, install_network_guard, merge_formulae, text_document


class FakeResult:
    def __init__(self, value):
        self.json = value


class FakeEngine:
    def recognise(self, path: Path, include_math: bool):
        document = text_document(
            [FakeResult({"page_index": 0, "rec_texts": ["로컬 문서"], "rec_scores": [0.99]})]
        )
        if include_math:
            return merge_formulae(
                document,
                [FakeResult({"page_index": 0, "formula_res_list": [{"rec_formula": "x^2"}]})],
            )
        return document


class ProtocolTests(unittest.TestCase):
    def test_unwraps_current_paddle_result_shape(self) -> None:
        document = text_document(
            [
                {
                    "res": {
                        "page_index": None,
                        "rec_texts": ["EveryFile OCR"],
                        "rec_scores": [0.99],
                        "rec_polys": [[[0, 0], [1, 0], [1, 1], [0, 1]]],
                    }
                }
            ]
        )
        self.assertEqual(document["plainText"], "EveryFile OCR")
        self.assertEqual(document["blocks"][0]["page"], 1)

    def test_accepts_supported_local_file_and_math_mode(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "scan.png"
            path.write_bytes(b"fixture")
            response = handle_request(
                {
                    "id": "ocr-1",
                    "operation": "ocr",
                    "path": str(path),
                    "mode": "math",
                    "maxBytes": 1024,
                },
                FakeEngine(),
            )
        self.assertTrue(response["ok"])
        self.assertIn("로컬 문서", response["document"]["plainText"])
        self.assertIn("x^2", response["document"]["markdown"])
        self.assertTrue(response["document"]["metadata"]["localOnly"])

    def test_rejects_unsupported_and_oversized_inputs(self):
        with tempfile.TemporaryDirectory() as directory:
            unsupported = Path(directory) / "scan.gif"
            unsupported.write_bytes(b"x")
            too_large = Path(directory) / "scan.jpg"
            too_large.write_bytes(b"xx")
            first = handle_request(
                {"id": "a", "operation": "ocr", "path": str(unsupported), "mode": "text", "maxBytes": 10},
                FakeEngine(),
            )
            second = handle_request(
                {"id": "b", "operation": "ocr", "path": str(too_large), "mode": "text", "maxBytes": 1},
                FakeEngine(),
            )
        self.assertEqual(first["error"]["code"], "UNSUPPORTED")
        self.assertEqual(second["error"]["code"], "TOO_LARGE")

    def test_network_guard_blocks_external_connections(self):
        install_network_guard()
        import socket

        with self.assertRaises(OSError):
            socket.create_connection(("example.com", 443), timeout=0.01)


if __name__ == "__main__":
    unittest.main()
