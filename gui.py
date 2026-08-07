#!/usr/bin/env python3
"""Fenix -> TFDI 导航数据转换工具图形界面。"""

from __future__ import annotations

import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import threading
import tkinter as tk
from tkinter import filedialog, messagebox, ttk

from gui_logic import (
    build_conversion_command,
    candidate_output_path,
    detect_paths,
    find_converter_executable,
    validate_conversion_paths,
)
from update_manager import (
    UpdateError,
    check_for_update,
    download_update,
    launch_update_installer,
)
from version import __version__


APP_TITLE = "Fenix -> TFDI 导航数据转换工具"
APP_VERSION = f"{__version__} 测试版"


class ConversionGUI:
    def __init__(self, update_result: dict | None = None) -> None:
        self.app_dir = Path(__file__).resolve().parent
        self.root = tk.Tk()
        self.root.title(f"{APP_TITLE} - {APP_VERSION}")
        self.root.geometry("860x650")
        self.root.minsize(760, 580)
        self.root.protocol("WM_DELETE_WINDOW", self.close)

        self.database_var = tk.StringVar()
        self.route_var = tk.StringVar()
        self.reference_var = tk.StringVar()
        self.output_var = tk.StringVar(value=str(self._new_output_path()))
        self.status_var = tk.StringVar(value="准备就绪")
        self.process: subprocess.Popen[str] | None = None
        self.worker: threading.Thread | None = None
        self.events: queue.Queue[tuple[str, object]] = queue.Queue()
        self.update_busy = False
        self.update_installing = False

        self._build_ui()
        self.root.after(300, lambda: self._run_startup_tasks(update_result))

    def _build_ui(self) -> None:
        title = ttk.Frame(self.root, padding=(14, 12, 14, 8))
        title.pack(fill=tk.X)
        ttk.Label(
            title,
            text=APP_TITLE,
            font=("Microsoft YaHei UI", 16, "bold"),
        ).pack(anchor=tk.W)
        ttk.Label(
            title,
            text="将 Fenix / NAIP 数据转换为 TFDI MD-11 Nav-Primary",
            font=("Microsoft YaHei UI", 9),
            foreground="#555555",
        ).pack(anchor=tk.W, pady=(2, 0))

        warning = tk.Frame(self.root, bg="#fff3cd", padx=12, pady=9)
        warning.pack(fill=tk.X, padx=14, pady=(0, 8))
        tk.Label(
            warning,
            text="开发测试版 · 未经 TFDI MD-11 实机验证",
            bg="#fff3cd",
            fg="#8a4b08",
            font=("Microsoft YaHei UI", 10, "bold"),
        ).pack(anchor=tk.W)
        tk.Label(
            warning,
            text="本工具只生成新的隔离候选目录，不会自动覆盖游戏 WASM 数据。",
            bg="#fff3cd",
            fg="#6b4a16",
            font=("Microsoft YaHei UI", 9),
        ).pack(anchor=tk.W, pady=(2, 0))

        ttk.Label(
            self.root,
            textvariable=self.status_var,
            relief=tk.SUNKEN,
            anchor=tk.W,
            padding=5,
        ).pack(fill=tk.X, side=tk.BOTTOM)

        actions = ttk.Frame(self.root, padding=(14, 8))
        actions.pack(fill=tk.X, side=tk.BOTTOM)
        self.start_button = ttk.Button(
            actions, text="开始转换", command=self.start_conversion
        )
        self.start_button.pack(side=tk.LEFT, padx=(0, 6))
        self.stop_button = ttk.Button(
            actions, text="停止", command=self.stop_conversion, state=tk.DISABLED
        )
        self.stop_button.pack(side=tk.LEFT, padx=6)
        self.open_button = ttk.Button(
            actions, text="打开输出目录", command=self.open_output, state=tk.DISABLED
        )
        self.open_button.pack(side=tk.LEFT, padx=6)
        self.update_button = ttk.Button(
            actions, text="检查更新", command=lambda: self.check_updates(manual=True)
        )
        self.update_button.pack(side=tk.LEFT, padx=6)
        ttk.Button(actions, text="退出", command=self.close).pack(side=tk.RIGHT)

        paths = ttk.LabelFrame(self.root, text=" 文件路径 ", padding=10)
        paths.pack(fill=tk.X, padx=14, pady=5)
        self.detect_button = ttk.Button(
            paths, text="自动检测路径", command=self.auto_detect
        )
        self.detect_button.grid(row=0, column=1, sticky=tk.W, padx=5, pady=(0, 7))

        self._path_row(
            paths,
            1,
            "Fenix nd.db3：",
            self.database_var,
            "选择 Fenix nd.db3",
            self._browse_database,
        )
        self._path_row(
            paths,
            2,
            "NAIP RTE_SEG.csv：",
            self.route_var,
            "选择 RTE_SEG.csv",
            self._browse_route,
        )
        self._path_row(
            paths,
            3,
            "TFDI 官方模板：",
            self.reference_var,
            "选择 Nav-Primary 目录",
            self._browse_reference,
        )
        self._path_row(
            paths,
            4,
            "候选输出位置：",
            self.output_var,
            "选择输出位置",
            self._browse_output_parent,
            readonly=True,
        )
        paths.columnconfigure(1, weight=1)

        ttk.Label(
            paths,
            text="固定目录名为 Nav-Primary；请选择其中尚未存在该目录的输出位置。",
            foreground="#666666",
        ).grid(row=5, column=1, columnspan=2, sticky=tk.W, padx=5, pady=(5, 0))

        progress = ttk.Frame(self.root, padding=(14, 5))
        progress.pack(fill=tk.X)
        self.progress = ttk.Progressbar(progress, mode="indeterminate")
        self.progress.pack(side=tk.LEFT, fill=tk.X, expand=True, padx=(0, 10))
        self.progress_label = ttk.Label(progress, text="等待开始", width=16)
        self.progress_label.pack(side=tk.RIGHT)

        log_frame = ttk.LabelFrame(self.root, text=" 运行日志 ", padding=5)
        log_frame.pack(fill=tk.BOTH, expand=True, padx=14, pady=5)
        self.log_text = tk.Text(
            log_frame,
            wrap=tk.WORD,
            height=10,
            font=("Consolas", 9),
            bg="#1e1e1e",
            fg="#d4d4d4",
            insertbackground="#d4d4d4",
            state=tk.DISABLED,
        )
        scrollbar = ttk.Scrollbar(
            log_frame, orient=tk.VERTICAL, command=self.log_text.yview
        )
        self.log_text.configure(yscrollcommand=scrollbar.set)
        self.log_text.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)
        scrollbar.pack(side=tk.RIGHT, fill=tk.Y)

    def _path_row(
        self,
        parent: ttk.LabelFrame,
        row: int,
        label: str,
        variable: tk.StringVar,
        button_text: str,
        command,
        readonly: bool = False,
    ) -> None:
        ttk.Label(parent, text=label, width=19, anchor=tk.E).grid(
            row=row, column=0, sticky=tk.E, pady=3
        )
        ttk.Entry(
            parent,
            textvariable=variable,
            state="readonly" if readonly else "normal",
        ).grid(
            row=row, column=1, sticky=tk.EW, padx=5, pady=3
        )
        ttk.Button(parent, text=button_text, command=command, width=20).grid(
            row=row, column=2, pady=3
        )

    def _browse_database(self) -> None:
        path = filedialog.askopenfilename(
            title="选择 Fenix nd.db3",
            filetypes=(("Fenix 数据库", "*.db3"), ("所有文件", "*.*")),
        )
        if path:
            self.database_var.set(path)

    def _browse_route(self) -> None:
        path = filedialog.askopenfilename(
            title="选择 RTE_SEG.csv",
            filetypes=(("航路数据", "*.csv"), ("所有文件", "*.*")),
        )
        if path:
            self.route_var.set(path)

    def _browse_reference(self) -> None:
        path = filedialog.askdirectory(title="选择 TFDI 官方 Nav-Primary 目录")
        if path:
            self.reference_var.set(path)

    def _browse_output_parent(self) -> None:
        path = filedialog.askdirectory(title="选择候选输出所在位置")
        if path:
            self.output_var.set(str(candidate_output_path(path)))

    def auto_detect(self) -> None:
        detected = detect_paths(self.app_dir)
        found: list[str] = []
        if detected.database:
            self.database_var.set(str(detected.database))
            found.append("Fenix 数据库")
        if detected.route_segments:
            self.route_var.set(str(detected.route_segments))
            found.append("航路 CSV")
        if detected.reference:
            self.reference_var.set(str(detected.reference))
            found.append("TFDI 模板")

        if found:
            summary = "、".join(found)
            self.status_var.set(f"已自动检测：{summary}")
            self.log(f"[自动检测] 已找到：{summary}\n")
        else:
            self.status_var.set("未自动找到输入，请手动选择")
            self.log("[自动检测] 未找到可用路径，请手动选择。\n")

    def _run_startup_tasks(self, update_result: dict | None) -> None:
        self.auto_detect()
        if update_result:
            self._show_update_result(update_result)
        self.check_updates(manual=False)

    def check_updates(self, manual: bool) -> None:
        """在后台检查 GitHub Release，不影响当前转换。"""
        if self.process is not None or self.update_busy:
            if manual and self.process is not None:
                messagebox.showinfo("检查更新", "请等待当前转换结束后再检查更新。")
            return
        self.update_busy = True
        self.update_button.configure(state=tk.DISABLED)
        if manual:
            self.status_var.set("正在检查更新……")
            self.progress_label.configure(text="正在检查更新……")

        def worker() -> None:
            result = check_for_update()
            self.root.after(0, lambda: self._on_update_checked(result, manual))

        threading.Thread(target=worker, daemon=True).start()

    def _on_update_checked(self, result, manual: bool) -> None:
        self.update_busy = False
        self.update_button.configure(state=tk.NORMAL if self.process is None else tk.DISABLED)
        if result.error:
            self.log(f"[更新检查] {result.error}\n")
            if manual:
                self.status_var.set("检查更新失败")
                self.progress_label.configure(text="检查更新失败")
                messagebox.showerror(
                    "检查更新",
                    "无法访问 GitHub 更新服务，请检查网络或稍后重试。",
                )
            return
        if not result.update_available:
            if manual:
                self.status_var.set(f"当前已是最新版 v{__version__}")
                self.progress_label.configure(text="当前已是最新版")
                messagebox.showinfo("检查更新", f"当前已是最新版 v{__version__}。")
            return
        release = result.release
        if not release:
            return
        if messagebox.askyesno(
            "发现新版本",
            f"发现新版本 v{release.version}（当前 v{__version__}）。\n\n"
            "是否立即下载安装？\n\n"
            "程序文件会先备份，安装成功后自动重启；安装失败会恢复原文件。",
        ):
            self._download_and_install_update(release)

    def _download_and_install_update(self, release) -> None:
        self.update_busy = True
        self.update_installing = True
        self.start_button.configure(state=tk.DISABLED)
        self.detect_button.configure(state=tk.DISABLED)
        self.update_button.configure(state=tk.DISABLED)
        self.progress.stop()
        self.progress.configure(mode="determinate", maximum=100, value=0)
        self.progress_label.configure(text=f"正在下载 v{release.version}……")
        self.status_var.set("正在下载并校验更新包……")

        def progress(received: int, total: int) -> None:
            percentage = min(100.0, received * 100.0 / max(total, 1))
            self.root.after(0, lambda: self.progress.configure(value=percentage))
            self.root.after(
                0,
                lambda: self.progress_label.configure(
                    text=(f"下载更新 {received / 1024 / 1024:.1f}/"
                          f"{total / 1024 / 1024:.1f} MB")
                ),
            )

        def worker() -> None:
            try:
                package_path = download_update(release, progress_callback=progress)
                self.root.after(
                    0, lambda: self._start_update_installer(package_path, release)
                )
            except (UpdateError, OSError) as error:
                self.root.after(0, lambda: self._on_update_failed(str(error)))

        threading.Thread(target=worker, daemon=True).start()

    def _start_update_installer(self, package_path: Path, release) -> None:
        try:
            self.progress.configure(value=100)
            self.progress_label.configure(text="下载校验完成，正在安装……")
            self.status_var.set("程序即将关闭，更新完成后会自动重启……")
            launch_update_installer(package_path, self.app_dir, release, os.getpid())
        except (UpdateError, OSError) as error:
            package_path.unlink(missing_ok=True)
            self._on_update_failed(str(error))
            return
        self.root.after(100, self.root.destroy)

    def _on_update_failed(self, error: str) -> None:
        self.update_busy = False
        self.update_installing = False
        self.start_button.configure(state=tk.NORMAL)
        self.detect_button.configure(state=tk.NORMAL)
        self.update_button.configure(state=tk.NORMAL)
        self.progress.configure(mode="indeterminate", value=0)
        self.progress_label.configure(text="自动更新失败")
        self.status_var.set("自动更新失败")
        self.log(f"[自动更新失败] {error}\n")
        messagebox.showerror("自动更新失败", f"更新包下载或校验失败，当前程序未被修改。\n\n{error}")

    def _show_update_result(self, result: dict) -> None:
        success = bool(result.get("success"))
        message = str(result.get("message") or "更新结果未知")
        if success:
            self.status_var.set(f"已成功更新到 v{result.get('version', __version__)}")
            self.progress_label.configure(text="自动更新完成")
            messagebox.showinfo("自动更新完成", message)
        else:
            self.status_var.set("自动更新失败，已保留原版本")
            self.progress_label.configure(text="自动更新失败")
            messagebox.showerror("自动更新失败", message)

    def start_conversion(self) -> None:
        if self.process is not None:
            return

        executable = find_converter_executable(self.app_dir)
        if executable is None:
            messagebox.showerror(
                "未找到转换程序",
                "未找到 fenix_to_tfdi.exe。\n\n"
                "请将已构建的程序放在 GUI 同目录，或先执行 Cargo release 构建。",
            )
            return

        database = Path(self.database_var.get().strip())
        route_segments = Path(self.route_var.get().strip())
        reference = Path(self.reference_var.get().strip())
        output = Path(self.output_var.get().strip())
        try:
            validate_conversion_paths(database, route_segments, reference, output)
        except ValueError as error:
            messagebox.showerror("路径无效", str(error))
            return

        if not messagebox.askyesno(
            "确认开始转换",
            "即将生成一份新的 TFDI 导航数据候选。\n\n"
            "当前版本未经 TFDI MD-11 实机验证，只能作为测试版使用。\n"
            "程序不会修改官方模板，也不会覆盖 WASM 活动数据。\n\n"
            f"候选输出：\n{output}\n\n是否继续？",
        ):
            return

        command = build_conversion_command(
            executable, database, route_segments, reference, output
        )
        self._set_running(True)
        self.open_button.configure(state=tk.DISABLED)
        self._clear_log()
        self.log("开发测试版 / 未经 TFDI MD-11 实机验证\n")
        self.log(f"输出目录：{output}\n\n")
        self.worker = threading.Thread(
            target=self._run_process, args=(command,), daemon=True
        )
        self.worker.start()
        self.root.after(100, self._poll_events)

    def _run_process(self, command: list[str]) -> None:
        try:
            creationflags = getattr(subprocess, "CREATE_NO_WINDOW", 0)
            process = subprocess.Popen(
                command,
                cwd=self.app_dir,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                encoding="utf-8",
                errors="replace",
                bufsize=1,
                creationflags=creationflags,
            )
            self.process = process
            assert process.stdout is not None
            for line in process.stdout:
                self.events.put(("log", line))
            exit_code = process.wait()
            self.events.put(("done", exit_code))
        except Exception as error:
            self.events.put(("error", str(error)))

    def _poll_events(self) -> None:
        while True:
            try:
                event, payload = self.events.get_nowait()
            except queue.Empty:
                break
            if event == "log":
                self.log(str(payload))
            elif event == "done":
                self._conversion_done(int(payload))
            elif event == "error":
                self._conversion_error(str(payload))

        worker_running = self.worker is not None and self.worker.is_alive()
        if worker_running or self.process is not None or not self.events.empty():
            self.root.after(100, self._poll_events)

    def _conversion_done(self, exit_code: int) -> None:
        self.process = None
        self._set_running(False)
        if exit_code == 0:
            self.progress_label.configure(text="转换及验证完成")
            self.status_var.set("候选数据已生成，并通过内置验证")
            self.open_button.configure(state=tk.NORMAL)
            messagebox.showinfo(
                "转换完成",
                "候选数据已生成，并通过转换器内置验证。\n\n"
                "注意：这不等于 TFDI MD-11 实机验证。",
            )
        else:
            self.progress_label.configure(text="转换失败")
            self.status_var.set(f"转换失败，退出代码：{exit_code}")
            messagebox.showerror("转换失败", "请查看运行日志中的详细错误。")

    def _conversion_error(self, error: str) -> None:
        self.process = None
        self._set_running(False)
        self.log(f"\n[启动失败] {error}\n")
        self.progress_label.configure(text="启动失败")
        self.status_var.set("无法启动转换程序")
        messagebox.showerror("启动失败", error)

    def stop_conversion(self) -> None:
        process = self.process
        if process is None:
            return
        if not messagebox.askyesno(
            "停止转换",
            "确定停止当前转换吗？未完成的候选目录不能使用，可在关闭程序后手动删除。",
        ):
            return
        process.terminate()
        self.status_var.set("正在停止转换……")

    def open_output(self) -> None:
        output = Path(self.output_var.get().strip())
        if not output.is_dir():
            messagebox.showerror("目录不存在", f"未找到候选输出目录：\n{output}")
            return
        os.startfile(output)  # type: ignore[attr-defined]

    def close(self) -> None:
        if self.process is not None:
            messagebox.showwarning("转换正在运行", "请先停止转换，再退出程序。")
            return
        if self.update_installing:
            messagebox.showwarning("更新正在进行", "请等待更新下载或安装完成。")
            return
        self.root.destroy()

    def _set_running(self, running: bool) -> None:
        if running:
            self.start_button.configure(state=tk.DISABLED)
            self.detect_button.configure(state=tk.DISABLED)
            self.stop_button.configure(state=tk.NORMAL)
            self.update_button.configure(state=tk.DISABLED)
            self.progress.start(12)
            self.progress_label.configure(text="正在转换……")
            self.status_var.set("正在生成并验证候选数据")
        else:
            self.start_button.configure(state=tk.NORMAL)
            self.detect_button.configure(state=tk.NORMAL)
            self.stop_button.configure(state=tk.DISABLED)
            self.update_button.configure(
                state=tk.DISABLED if self.update_busy else tk.NORMAL
            )
            self.progress.stop()

    def log(self, message: str) -> None:
        self.log_text.configure(state=tk.NORMAL)
        self.log_text.insert(tk.END, message)
        self.log_text.see(tk.END)
        self.log_text.configure(state=tk.DISABLED)

    def _clear_log(self) -> None:
        self.log_text.configure(state=tk.NORMAL)
        self.log_text.delete("1.0", tk.END)
        self.log_text.configure(state=tk.DISABLED)

    def _new_output_path(self) -> Path:
        return candidate_output_path(self.app_dir / "output")

    def run(self) -> None:
        self.root.mainloop()


def read_update_result(argv: list[str]) -> dict | None:
    """读取安装器写入的结果，并立即删除临时文件。"""
    if "--update-result" not in argv:
        return None
    index = argv.index("--update-result")
    if index + 1 >= len(argv):
        return {"success": False, "message": "更新结果文件参数无效"}
    path = Path(argv[index + 1])
    try:
        result = json.loads(path.read_text(encoding="utf-8"))
        return result if isinstance(result, dict) else None
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        return {"success": False, "message": f"无法读取更新结果: {error}"}
    finally:
        path.unlink(missing_ok=True)


def main() -> None:
    ConversionGUI(read_update_result(sys.argv[1:])).run()


if __name__ == "__main__":
    main()
