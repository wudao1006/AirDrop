import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ErrorToast } from "./ErrorToast";

afterEach(() => vi.useRealTimers());

describe("ErrorToast", () => {
  it("closes automatically after five seconds and still supports manual close", () => {
    vi.useFakeTimers();
    const onClose = vi.fn();
    render(<ErrorToast message="设备操作失败" onClose={onClose} />);
    act(() => vi.advanceTimersByTime(4_999));
    expect(onClose).not.toHaveBeenCalled();
    act(() => vi.advanceTimersByTime(1));
    expect(onClose).toHaveBeenCalledOnce();

    fireEvent.click(screen.getByRole("button", { name: "关闭" }));
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});
