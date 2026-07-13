import { useEffect } from "react";

export function ErrorToast({ message, onClose }: { message: string; onClose: () => void }) {
  useEffect(() => {
    const timer = window.setTimeout(onClose, 5_000);
    return () => window.clearTimeout(timer);
  }, [message, onClose]);

  return <div className="toast-error" role="alert">{message}<button type="button" onClick={onClose}>关闭</button></div>;
}
