import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { IndexStatus as IndexStatusModel } from "../../lib/types";
import { IndexStatus } from "./IndexStatus";

const parsing: IndexStatusModel = {
  jobId: "job-1",
  state: "parsing",
  totalFiles: 10,
  completedFiles: 4,
  currentPath: "C:\\Users\\me\\Private\\quarterly-report.pdf",
  errors: [{ code: "DAMAGED", fileName: "broken.pdf", message: "damaged" }],
};

describe("IndexStatus", () => {
  afterEach(cleanup);

  it("shows progress and only the current filename until details are expanded", () => {
    render(
      <IndexStatus
        status={parsing}
        onPause={() => undefined}
        onResume={() => undefined}
        onCancel={() => undefined}
      />,
    );

    expect(screen.getByRole("progressbar")).toHaveAttribute("value", "4");
    expect(screen.getByText("quarterly-report.pdf")).toBeVisible();
    expect(screen.queryByText(parsing.currentPath!)).not.toBeInTheDocument();
    expect(screen.getByText("오류 1개")).toBeVisible();

    fireEvent.click(screen.getByRole("button", { name: "경로 세부정보 보기" }));
    expect(screen.getByText(parsing.currentPath!)).toBeVisible();
  });

  it("routes pause, resume, and cancel using the job identifier", () => {
    const onPause = vi.fn();
    const onResume = vi.fn();
    const onCancel = vi.fn();
    const { rerender } = render(
      <IndexStatus
        status={parsing}
        onPause={onPause}
        onResume={onResume}
        onCancel={onCancel}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "일시정지" }));
    fireEvent.click(screen.getByRole("button", { name: "취소" }));
    expect(onPause).toHaveBeenCalledWith("job-1");
    expect(onCancel).toHaveBeenCalledWith("job-1");

    rerender(
      <IndexStatus
        status={{ ...parsing, state: "paused" }}
        onPause={onPause}
        onResume={onResume}
        onCancel={onCancel}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "계속" }));
    expect(onResume).toHaveBeenCalledWith("job-1");
  });
});
