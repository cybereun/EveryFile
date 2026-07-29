import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import * as CFB from "cfb";
import JSZip from "jszip";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";

import {
  createRequestHandler,
  handleLine,
  handleRequest,
  ParseResponseSchema,
  type ParseDependencies,
  type ParseRequest,
} from "../src/protocol.js";

const MAX_BYTES = 20_000_000;
let fixtureDirectory = "";

function fixturePath(name: string): string {
  return resolve(fixtureDirectory, name);
}

async function writeZip(path: string, zip: JSZip): Promise<void> {
  await writeFile(path, await zip.generateAsync({ type: "nodebuffer" }));
}

async function createHwpx(path: string): Promise<void> {
  const zip = new JSZip();
  zip.file(
    "Contents/content.hpf",
    `<?xml version="1.0" encoding="UTF-8"?>
<opf:package xmlns:opf="http://www.idpf.org/2007/opf">
  <opf:manifest><opf:item id="s0" href="section0.xml" media-type="application/xml"/></opf:manifest>
  <opf:spine><opf:itemref idref="s0"/></opf:spine>
</opf:package>`,
  );
  zip.file(
    "Contents/section0.xml",
    `<?xml version="1.0" encoding="UTF-8"?>
<hs:sec xmlns:hs="http://www.hancom.co.kr/hwpml/2016/HwpMl"
        xmlns:hp="http://www.hancom.co.kr/hwpml/2016/HwpMl">
  <hp:p><hp:run><hp:t>테스트 문서 HWPX</hp:t></hp:run></hp:p>
</hs:sec>`,
  );
  await writeZip(path, zip);
}

function hwpRecord(tagId: number, level: number, data: Buffer): Buffer {
  const header = Buffer.alloc(4);
  header.writeUInt32LE(tagId | (level << 10) | (data.length << 20));
  return Buffer.concat([header, data]);
}

async function createHwp(path: string): Promise<void> {
  const header = Buffer.alloc(256);
  header.write("HWP Document File", 0, "utf8");
  header[35] = 5;

  const paragraphHeader = Buffer.alloc(22);
  const paragraphText = Buffer.from("테스트 문서 HWP\r", "utf16le");
  const section = Buffer.concat([
    hwpRecord(0x42, 0, paragraphHeader),
    hwpRecord(0x43, 1, paragraphText),
  ]);

  const cfb = CFB.utils.cfb_new();
  CFB.utils.cfb_add(cfb, "FileHeader", header);
  CFB.utils.cfb_add(cfb, "BodyText/Section0", section);
  const output = CFB.write(cfb, { type: "buffer" });
  await writeFile(path, Buffer.from(output));
}

function createPdf(text: string): Buffer {
  const objects = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    `<< /Length ${text.length + 34} >>\nstream\nBT /F1 18 Tf 72 720 Td (${text}) Tj ET\nendstream`,
  ];
  let pdf = "%PDF-1.4\n";
  const offsets = [0];
  objects.forEach((object, index) => {
    offsets.push(Buffer.byteLength(pdf));
    pdf += `${index + 1} 0 obj\n${object}\nendobj\n`;
  });
  const xrefOffset = Buffer.byteLength(pdf);
  pdf += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
  for (const offset of offsets.slice(1)) {
    pdf += `${offset.toString().padStart(10, "0")} 00000 n \n`;
  }
  pdf += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xrefOffset}\n%%EOF\n`;
  return Buffer.from(pdf, "ascii");
}

async function createXlsx(path: string): Promise<void> {
  const zip = new JSZip();
  zip.file("[Content_Types].xml", "<Types/>");
  zip.file(
    "xl/workbook.xml",
    `<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"
      xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
      <sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets>
    </workbook>`,
  );
  zip.file(
    "xl/_rels/workbook.xml.rels",
    `<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
      <Relationship Id="rId1" Target="worksheets/sheet1.xml"/>
    </Relationships>`,
  );
  zip.file(
    "xl/worksheets/sheet1.xml",
    `<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
      <sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>테스트 문서 XLSX</t></is></c></row></sheetData>
    </worksheet>`,
  );
  await writeZip(path, zip);
}

async function createDocx(path: string): Promise<void> {
  const zip = new JSZip();
  zip.file(
    "[Content_Types].xml",
    `<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
      <Override PartName="/word/document.xml"
        ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
    </Types>`,
  );
  zip.file(
    "_rels/.rels",
    `<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
      <Relationship Id="rId1"
        Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument"
        Target="word/document.xml"/>
    </Relationships>`,
  );
  zip.file(
    "word/document.xml",
    `<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
      <w:body><w:p><w:r><w:t>테스트 문서 DOCX</w:t></w:r></w:p></w:body>
    </w:document>`,
  );
  zip.file(
    "word/_rels/document.xml.rels",
    `<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>`,
  );
  await writeZip(path, zip);
}

beforeAll(async () => {
  fixtureDirectory = await mkdtemp(join(tmpdir(), "everyfile-parser-"));
  await Promise.all([
    createHwpx(fixturePath("simple.hwpx")),
    createHwp(fixturePath("simple.hwp")),
    writeFile(fixturePath("simple.pdf"), createPdf("EveryFile PDF fixture")),
    createXlsx(fixturePath("simple.xlsx")),
    createDocx(fixturePath("simple.docx")),
    writeFile(fixturePath("unsupported.bin"), Buffer.from([1, 2, 3, 4])),
  ]);
});

afterAll(async () => {
  await rm(fixtureDirectory, { recursive: true, force: true });
});

function request(path: string, id = "req-1"): ParseRequest {
  return {
    id,
    operation: "parse",
    path,
    options: { maxBytes: MAX_BYTES },
  };
}

async function parseFixture(
  name: string,
  options?: { timeoutMs?: number },
) {
  if (name === "slow.pdf") {
    const dependencies: Partial<ParseDependencies> = {
      timeoutMs: options?.timeoutMs,
      parseDocument: () => new Promise(() => undefined),
    };
    return handleRequest(request(fixturePath("simple.pdf")), dependencies);
  }
  return handleRequest(request(fixturePath(name)));
}

describe("parser protocol", () => {
  it("normalizes a successful Kordoc result", async () => {
    const response = await handleRequest(request(fixturePath("simple.hwpx")));
    expect(response).toMatchObject({
      id: "req-1",
      ok: true,
      document: { parserKind: "kordoc", warnings: [] },
    });
    expect(response.ok && response.document.plainText).toContain("테스트 문서");
  });

  it("rejects malformed JSON and unsupported operations", async () => {
    expect(await handleLine("{bad json")).toMatchObject({
      ok: false,
      error: { code: "INVALID_REQUEST" },
    });
    expect(
      await handleLine(JSON.stringify({ id: "x", operation: "erase" })),
    ).toMatchObject({
      ok: false,
      error: { code: "INVALID_REQUEST" },
    });
  });

  it("returns typed unsupported and timeout errors", async () => {
    expect((await parseFixture("unsupported.bin")).error?.code).toBe(
      "UNSUPPORTED",
    );
    await expect(
      parseFixture("slow.pdf", { timeoutMs: 10 }),
    ).resolves.toMatchObject({
      ok: false,
      error: { code: "TIMEOUT" },
    });
  });

  it.each([
    ["simple.hwpx", "테스트 문서 HWPX"],
    ["simple.hwp", "테스트 문서 HWP"],
    ["simple.pdf", "EveryFile PDF fixture"],
    ["simple.xlsx", "테스트 문서 XLSX"],
    ["simple.docx", "테스트 문서 DOCX"],
  ])("normalizes %s text", async (name, expectedText) => {
    const response = await parseFixture(name);
    expect(response).toMatchObject({
      ok: true,
      document: { parserKind: "kordoc" },
    });
    expect(response.ok && response.document.plainText).toContain(expectedText);
  });

  it("enforces the declared maximum size before parsing", async () => {
    const response = await handleRequest({
      ...request(fixturePath("unsupported.bin")),
      options: { maxBytes: 1 },
    });
    expect(response).toMatchObject({
      ok: false,
      error: { code: "TOO_LARGE" },
    });
  });

  it("emits runtime-valid exclusive responses without leaking details", async () => {
    const secret = "TOP_SECRET_VALUE";
    const diagnostic = vi
      .spyOn(process.stderr, "write")
      .mockImplementation(() => true);
    const response = await handleRequest(request(fixturePath("simple.hwpx")), {
      parseDocument: async () => {
        throw new Error(`${secret} at C:\\Users\\private\\document.hwp`);
      },
    });
    expect(() => ParseResponseSchema.parse(response)).not.toThrow();
    expect(response).toMatchObject({
      ok: false,
      error: { code: "INTERNAL" },
    });
    expect(JSON.stringify(response)).not.toContain(secret);
    expect(JSON.stringify(response)).not.toContain("C:\\Users");
    expect(diagnostic).not.toHaveBeenCalledWith(expect.stringContaining(secret));
    expect("document" in response).toBe(false);
    diagnostic.mockRestore();
  });

  it("processes no more than three requests concurrently", async () => {
    let active = 0;
    let maximum = 0;
    const handler = createRequestHandler({
      parseDocument: async () => {
        active += 1;
        maximum = Math.max(maximum, active);
        await new Promise((resolvePromise) => setTimeout(resolvePromise, 15));
        active -= 1;
        return {
          ok: true,
          document: {
            parserKind: "kordoc",
            title: null,
            markdown: "ok",
            plainText: "ok",
            blocks: [],
            metadata: {},
            warnings: [],
          },
        };
      },
    });

    await Promise.all(
      Array.from({ length: 8 }, (_, index) =>
        handler(JSON.stringify(request(fixturePath("simple.hwpx"), `r-${index}`))),
      ),
    );
    expect(maximum).toBe(3);
  });
});
