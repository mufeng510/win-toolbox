using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Globalization;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using System.Windows.Forms;

namespace HeicToJpg
{
    internal static class Program
    {
        [DllImport("shcore.dll")]
        private static extern int GetProcessDpiAwareness(IntPtr h, out int awareness);

        [DllImport("shcore.dll")]
        private static extern int GetDpiForSystem();

        [DllImport("user32.dll")]
        internal static extern uint GetDpiForWindow(IntPtr h);
        [STAThread]
        private static int Main(string[] args)
        {
            // DPI 感知由 app.manifest（PerMonitorV2）声明

            // 布局探针：HeicToJpg.exe --probe
if (args.Length >= 1 && args[0] == "--probe")
{
    try
    {
        MainForm f = new MainForm(null);
        IntPtr h = f.Handle;                       // 强制创建句柄
        f.CreateControl();
        typeof(Form).GetMethod("OnLoad", System.Reflection.BindingFlags.Instance |
                                     System.Reflection.BindingFlags.NonPublic)
                   .Invoke(f, new object[] { EventArgs.Empty });
        StringBuilder sb = new StringBuilder();
        int aware;
        try { GetProcessDpiAwareness(System.Diagnostics.Process.GetCurrentProcess().Handle, out aware); }
        catch { aware = -1; }
        sb.AppendLine("DPI 感知级别   = " + aware + "  (0=无 1=系统 2=PerMonitor)");
        try { sb.AppendLine("GetDpiForSystem= " + GetDpiForSystem()); } catch { }
        sb.AppendLine("DeviceDpi      = " + f.DeviceDpi);
        sb.AppendLine("AutoScaleFactor= (protected, 见下方 ClientSize)");
        sb.AppendLine("form.ClientSize = " + f.ClientSize);
        sb.AppendLine("form.Size       = " + f.Size);
        sb.AppendLine("form.Font.Height= " + f.Font.Height);
        foreach (Control c in f.Controls)
            sb.AppendLine("  " + c.GetType().Name.PadRight(11) + " '" + c.Text + "' Bounds=" + c.Bounds);
        GroupBox g = f.Controls["grpOptions"] as GroupBox;
        sb.AppendLine("--- grpOptions 内部 ---");
        if (g != null)
            foreach (Control c in g.Controls)
                sb.AppendLine("  " + c.GetType().Name.PadRight(11) + " '" + c.Text + "' Bounds=" + c.Bounds);
        string lp = Path.Combine(Path.GetTempPath(), "HeicToJpg_probe.txt");
        File.WriteAllText(lp, sb.ToString(), Encoding.UTF8);
        Console.Out.Write(sb.ToString());
        return 0;
    }
    catch (Exception ex)
    {
        File.WriteAllText(Path.Combine(Path.GetTempPath(), "HeicToJpg_probe.txt"), ex.ToString(), Encoding.UTF8);
        return 3;
    }
}

// 无界面自检模式：HeicToJpg.exe --selftest "<目录>" <质量> <0|1递归> <0|1删除>
            if (args.Length >= 2 && args[0] == "--selftest")
            {
                int q = args.Length > 2 ? int.Parse(args[2], CultureInfo.InvariantCulture) : 95;
                bool rec = args.Length > 3 && args[3] == "1";
                bool del = args.Length > 4 && args[4] == "1";
                try { Console.OutputEncoding = Encoding.UTF8; } catch { }
                StringBuilder sb = new StringBuilder();
                int r = Converter.Run(args[1], q, rec, del,
                    delegate (string m) { lock (sb) sb.AppendLine(m); },
                    delegate (int i, int n) { });
                Console.Out.Write(sb.ToString());
                Console.Out.Flush();
                return r;
            }

            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            try
            {
                Application.Run(new MainForm(args.Length > 0 ? args[0] : null));
            }
            catch (Exception ex)
            {
                string lp = Path.Combine(Path.GetTempPath(), "HeicToJpg_error.log");
                File.WriteAllText(lp, ex.ToString(), Encoding.UTF8);
                MessageBox.Show(ex.Message + "\n\n日志: " + lp, "HeicToJpg 启动失败",
                                MessageBoxButtons.OK, MessageBoxIcon.Error);
            }
            return 0;
        }
    }

    internal sealed class ConvertOptions
    {
        public int Quality = 95;
        public bool Recursive = true;
        public bool DeleteSource = true;
    }

    internal sealed class ConvertResult
    {
        public int Total;
        public int Succeeded;
        public int Skipped;
        public int Failed;
        public long BytesOut;
        public long BytesFreed;
    }

    internal static class Converter
    {
        private static readonly string[] HeicExt = { ".heic", ".heif" };

        public static string FindMagick()
        {
            string env = Environment.GetEnvironmentVariable("HEIC2JPG_MAGICK");
            if (!string.IsNullOrEmpty(env) && File.Exists(env)) return env;

            string exeDir = AppDomain.CurrentDomain.BaseDirectory;
            string local = Path.Combine(exeDir, "magick.exe");
            if (File.Exists(local)) return local;

            string searchRoot = Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles);
            List<string> roots = new List<string> { searchRoot };
            roots.Add(@"C:\Program Files");
            roots.Add(@"C:\Program Files (x86)");
            roots.Add(@"C:\Tools");
            foreach (string root in roots)
            {
                if (string.IsNullOrEmpty(root) || !Directory.Exists(root)) continue;
                try
                {
                    string[] dirs = Directory.GetDirectories(root, "ImageMagick*");
                    Array.Sort(dirs, StringComparer.OrdinalIgnoreCase);
                    Array.Reverse(dirs);
                    foreach (string d in dirs)
                    {
                        string p = Path.Combine(d, "magick.exe");
                        if (File.Exists(p)) return p;
                    }
                }
                catch { }
            }

            string pathVar = Environment.GetEnvironmentVariable("PATH") ?? string.Empty;
            foreach (string p in pathVar.Split(Path.PathSeparator))
            {
                try
                {
                    string cand = Path.Combine(p.Trim().Trim('"'), "magick.exe");
                    if (File.Exists(cand)) return cand;
                }
                catch { }
            }
            return null;
        }

        public static int Count(string dir, bool recursive)
        {
            try { return FindFiles(dir, recursive).Count; }
            catch { return 0; }
        }

        public static List<string> FindFiles(string dir, bool recursive)
        {
            SearchOption opt = recursive ? SearchOption.AllDirectories : SearchOption.TopDirectoryOnly;
            string[] files = Directory.GetFiles(dir, "*", opt);
            List<string> list = new List<string>();
            foreach (string f in files)
            {
                if (Array.IndexOf(HeicExt, Path.GetExtension(f).ToLowerInvariant()) >= 0)
                    list.Add(f);
            }
            list.Sort(StringComparer.OrdinalIgnoreCase);
            return list;
        }

        private static string TargetPath(string source)
        {
            return Path.Combine(Path.GetDirectoryName(source),
                                Path.GetFileNameWithoutExtension(source) + ".jpg");
        }

        private static string Run(string magick, string args, out string error)
        {
            error = null;
            ProcessStartInfo psi = new ProcessStartInfo(magick, args);
            psi.UseShellExecute = false;
            psi.CreateNoWindow = true;
            psi.RedirectStandardOutput = true;
            psi.RedirectStandardError = true;
            StringBuilder sb = new StringBuilder();
            using (Process proc = Process.Start(psi))
            {
                proc.OutputDataReceived += delegate (object s, DataReceivedEventArgs e) { if (e.Data != null) lock (sb) sb.AppendLine(e.Data); };
                proc.ErrorDataReceived += delegate (object s, DataReceivedEventArgs e) { if (e.Data != null) lock (sb) sb.AppendLine(e.Data); };
                proc.BeginOutputReadLine();
                proc.BeginErrorReadLine();
                proc.WaitForExit();
                lock (sb) error = sb.ToString().Trim();
                return proc.ExitCode.ToString(CultureInfo.InvariantCulture);
            }
        }

        public static int Run(string dir, int quality, bool recursive, bool deleteSource,
                              Action<string> log, Action<int, int> progress)
        {
            log = log ?? delegate { };
            string magick = FindMagick();
            if (magick == null) { log("✗ 找不到 magick.exe（ImageMagick）。请重新安装并勾选加入 PATH。"); return 2; }

            List<string> files;
            try { files = FindFiles(dir, recursive); }
            catch (Exception ex) { log("✗ 读取目录失败: " + ex.Message); return 2; }

            ConvertResult r = new ConvertResult();
            r.Total = files.Count;
            if (r.Total == 0) { log("该目录没有 HEIC/HEIF 文件。"); return 0; }

            log(string.Format(CultureInfo.InvariantCulture,
                "找到 {0} 个 HEIC/HEIF 文件，质量 {1}{2}{3}", r.Total, quality,
                recursive ? "，含子文件夹" : string.Empty,
                deleteSource ? "，成功后删除原文件" : string.Empty));
            log(new string('-', 46));

            int i = 0;
            foreach (string src in files)
            {
                i++;
                if (progress != null) progress(i, r.Total);
                string name = Path.GetFileName(src);
                string dest = TargetPath(src);

                if (File.Exists(dest) && new FileInfo(dest).Length > 0)
                {
                    r.Skipped++;
                    log("· 跳过（同名 JPG 已存在）: " + name);
                    continue;
                }

                long srcLen = 0;
                try { srcLen = new FileInfo(src).Length; } catch { }

                string q = "\"" + src + "\"";
                string o = "\"" + dest + "\"";
                string err;
                string code = Run(magick, q + " -auto-orient -strip -quality " +
                                  quality.ToString(CultureInfo.InvariantCulture) + " " + o, out err);
                if (code != "0" || !File.Exists(dest) || new FileInfo(dest).Length == 0)
                {
                    r.Failed++;
                    log("✗ 失败: " + name + (string.IsNullOrEmpty(err) ? "" : "  " + FirstLine(err)));
                    continue;
                }

                // 删除原文件前再验证一次输出确实可正常读取
                string verr;
                string vcode = Run(magick, "identify \"" + dest + "\"", out verr);
                if (vcode != "0")
                {
                    r.Failed++;
                    try { File.Delete(dest); } catch { }
                    log("✗ 输出校验失败，已删除损坏文件: " + name);
                    continue;
                }

                long dstLen = new FileInfo(dest).Length;
                r.Succeeded++;
                r.BytesOut += dstLen;

                if (deleteSource)
                {
                    try
                    {
                        File.Delete(src);
                        r.BytesFreed += srcLen;
                        log(string.Format(CultureInfo.InvariantCulture,
                            "[{0}/{1}] {2}  →  {3} KB  (原文件已删除)", i, r.Total, name, dstLen / 1024));
                    }
                    catch (Exception ex)
                    {
                        log("[! ] " + name + " 转换成功，但删除原文件失败: " + ex.Message);
                    }
                }
                else
                {
                    log(string.Format(CultureInfo.InvariantCulture,
                        "[{0}/{1}] {2}  →  {3} KB", i, r.Total, name, dstLen / 1024));
                }
            }

            log(new string('-', 46));
            log(string.Format(CultureInfo.InvariantCulture,
                "成功 {0} · 跳过 {1} · 失败 {2} / 共 {3}",
                r.Succeeded, r.Skipped, r.Failed, r.Total));
            log(string.Format(CultureInfo.InvariantCulture,
                "JPG 体积 {0:N0} MB，删除原文件释放 {1:N0} MB",
                r.BytesOut / 1048576.0, r.BytesFreed / 1048576.0));
            return r.Failed > 0 ? 1 : 0;
        }

        private static string FirstLine(string s)
        {
            int i = s.IndexOfAny(new char[] { '\r', '\n' });
            return i < 0 ? s : s.Substring(0, i);
        }
    }

    internal sealed class MainForm : Form
    {
        private readonly TextBox txtPath = new TextBox();
        private readonly Button btnBrowse = new Button();
        private readonly Button btnOpen = new Button();
        private readonly NumericUpDown numQuality = new NumericUpDown();
        private readonly CheckBox chkRecursive = new CheckBox();
        private readonly CheckBox chkDelete = new CheckBox();
        private readonly Button btnStart = new Button();
        private readonly Button btnClose = new Button();
        private readonly ProgressBar bar = new ProgressBar();
        private readonly Label lblStatus = new Label();
        private readonly TextBox txtLog = new TextBox();
        private readonly GroupBox grpOptions = new GroupBox();
        private string folder;

        public MainForm(string target)
        {
            Text = "批量 HEIC 转 JPG";
            FormBorderStyle = FormBorderStyle.FixedDialog;
            MaximizeBox = false;
            StartPosition = FormStartPosition.CenterScreen;
            ClientSize = new Size(640, 520);
            Font = MakeUiFont();
            // 不使用 WinForms 自动缩放：DPI 适配在 OnLoad 里按 DeviceDpi 精确计算
            AutoScaleMode = AutoScaleMode.None;

            string magick = Converter.FindMagick();
            if (magick != null)
            {
                try { Icon = Icon.ExtractAssociatedIcon(magick); } catch { }
            }

            folder = string.IsNullOrEmpty(target) ? DefaultFolder() : target;

            var lblTarget = new Label
            {
                Text = "目标文件夹", AutoSize = true, Location = new Point(16, 18),
                Anchor = AnchorStyles.Left | AnchorStyles.Top
            };

            txtPath.ReadOnly = true;
            txtPath.Multiline = true;          // 让高度可控，与旁边按钮对齐
            txtPath.BorderStyle = BorderStyle.FixedSingle;
            txtPath.BackColor = Color.FromArgb(246, 246, 246);
            txtPath.Location = new Point(16, 39);
            txtPath.Size = new Size(470, 28);
            txtPath.Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right;
            txtPath.Text = folder ?? string.Empty;
            txtPath.TextChanged += delegate { folder = txtPath.Text.Trim(); };

            btnBrowse.Text = "更改…";
            btnBrowse.Location = new Point(496, 39);
            btnBrowse.Size = new Size(62, 28);
            btnBrowse.Anchor = AnchorStyles.Top | AnchorStyles.Right;
            btnBrowse.Click += BrowseClicked;

            btnOpen.Text = "打开";
            btnOpen.Location = new Point(566, 39);
            btnOpen.Size = new Size(58, 28);
            btnOpen.Anchor = AnchorStyles.Top | AnchorStyles.Right;
            btnOpen.Click += delegate
            {
                try { if (Directory.Exists(folder)) Process.Start("explorer.exe", "\"" + folder + "\""); }
                catch { }
            };

            grpOptions.Text = "转换选项";
            grpOptions.Location = new Point(16, 78);
            grpOptions.Size = new Size(608, 92);
            grpOptions.Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right;

            var lblHint = new Label
            {
                Text = "高质量，体积最大", AutoSize = true, Location = new Point(16, 60),
                ForeColor = Color.FromArgb(110, 110, 110)
            };

            var lblQ = new Label { Text = "JPG 质量", AutoSize = true, Location = new Point(16, 30) };
            numQuality.Minimum = 1; numQuality.Maximum = 100; numQuality.Value = 95;
            numQuality.Location = new Point(88, 26);
            numQuality.Size = new Size(70, 26);
            numQuality.ValueChanged += delegate
            {
                int q = (int)numQuality.Value;
                lblHint.Text = q >= 93 ? "高质量，体积最大" : (q >= 88 ? "均衡，推荐用于上传" : "体积小，适合网页");
            };

            chkRecursive.Text = "包含子文件夹";
            chkRecursive.Checked = true;
            chkRecursive.AutoSize = true;
            chkRecursive.Location = new Point(190, 30);

            chkDelete.Text = "转换成功后删除原 HEIC 文件（不可撤销）";
            chkDelete.Checked = true;
            chkDelete.AutoSize = true;
            chkDelete.Location = new Point(320, 30);

            grpOptions.Controls.Add(lblQ);
            grpOptions.Controls.Add(numQuality);
            grpOptions.Controls.Add(chkRecursive);
            grpOptions.Controls.Add(chkDelete);
            grpOptions.Controls.Add(lblHint);

            bar.Location = new Point(16, 182);
            bar.Size = new Size(608, 20);
            bar.Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right;
            bar.Style = ProgressBarStyle.Continuous;

            lblStatus.Text = "就绪";
            lblStatus.Location = new Point(16, 208);
            lblStatus.AutoSize = true;
            lblStatus.ForeColor = Color.FromArgb(70, 70, 70);

            txtLog.Multiline = true;
            txtLog.ReadOnly = true;
            txtLog.ScrollBars = ScrollBars.Vertical;
            txtLog.Location = new Point(16, 234);
            txtLog.Size = new Size(608, 218);
            txtLog.Anchor = AnchorStyles.Top | AnchorStyles.Bottom | AnchorStyles.Left | AnchorStyles.Right;
            txtLog.BackColor = Color.FromArgb(250, 250, 250);
            txtLog.Font = new Font("Consolas", 9F);
            txtLog.WordWrap = false;

            btnStart.Text = "开始转换";
            btnStart.Location = new Point(16, 466);
            btnStart.Size = new Size(110, 34);
            btnStart.Anchor = AnchorStyles.Bottom | AnchorStyles.Left;
            btnStart.Click += StartClicked;

            btnClose.Text = "关闭";
            btnClose.Location = new Point(554, 466);
            btnClose.Size = new Size(70, 34);
            btnClose.Anchor = AnchorStyles.Bottom | AnchorStyles.Right;
            btnClose.Click += delegate { Close(); };

            Controls.Add(lblTarget);
            Controls.Add(txtPath);
            Controls.Add(btnBrowse);
            Controls.Add(btnOpen);
            Controls.Add(grpOptions);
            Controls.Add(bar);
            Controls.Add(lblStatus);
            Controls.Add(txtLog);
            Controls.Add(btnStart);
            Controls.Add(btnClose);
            AcceptButton = btnStart;

            Log("就绪。点击“开始转换”处理当前文件夹。");
            if (magick != null) Log("ImageMagick: " + magick);
            else Log("✗ 未检测到 ImageMagick，请先安装。");
        }

        private static string DefaultFolder()
        {
            try { return Environment.GetFolderPath(Environment.SpecialFolder.MyPictures); }
            catch { return null; }
        }

        private static Font MakeUiFont()
        {
            try { return new Font("Microsoft YaHei UI", 9F, FontStyle.Regular, GraphicsUnit.Point); }
            catch { return SystemFonts.MessageBoxFont; }
        }

        private bool dpiApplied;

        protected override void OnLoad(EventArgs e)
        {
            base.OnLoad(e);
            if (!dpiApplied) { dpiApplied = true; ApplyDpi(); }
        }

        // 按当前显示器的 DPI 精确缩放布局（本工程在 200% 缩放的 4K 屏上验证过）
        private void ApplyDpi()
        {
            int dpi = 0;
            try { dpi = (int)Program.GetDpiForWindow(Handle); } catch { dpi = 0; }
            if (dpi <= 0) dpi = DeviceDpi;
            float k = dpi / 96F;
            if (k <= 0.01F) k = 1F;
            PinAnchors(this);
            ClientSize = new Size((int)Math.Round(640 * k), (int)Math.Round(520 * k));
            ScaleTree(this, k);
            txtLog.Font = new Font("Consolas", 9F * k, FontStyle.Regular, GraphicsUnit.Point);
            Log(string.Format(CultureInfo.InvariantCulture, "显示器 DPI {0}（缩放 {1}%），布局已适配", dpi, (int)Math.Round(k * 100)));
        }

        private static void ScaleTree(Control root, float k)
        {
            foreach (Control c in root.Controls)
            {
                c.Bounds = new Rectangle(
                    (int)Math.Round(c.Left * k), (int)Math.Round(c.Top * k),
                    (int)Math.Round(c.Width * k), (int)Math.Round(c.Height * k));
                ScaleTree(c, k);
            }
        }

        // 窗口不可缩放，Anchor 只会干扰 DPI 布局计算，全部改为左上角固定
        private static void PinAnchors(Control root)
        {
            foreach (Control c in root.Controls)
            {
                c.Anchor = AnchorStyles.Top | AnchorStyles.Left;
                PinAnchors(c);
            }
        }

        private void BrowseClicked(object sender, EventArgs e)
        {
            using (FolderBrowserDialog dlg = new FolderBrowserDialog())
            {
                if (Directory.Exists(folder)) dlg.SelectedPath = folder;
                if (dlg.ShowDialog(this) == DialogResult.OK)
                {
                    txtPath.Text = dlg.SelectedPath;
                    folder = dlg.SelectedPath;
                }
            }
        }

        private void Log(string msg)
        {
            if (InvokeRequired) { BeginInvoke(new Action<string>(Log), msg); return; }
            txtLog.AppendText(msg + Environment.NewLine);
        }

        private void SetBusy(bool busy)
        {
            if (InvokeRequired) { BeginInvoke(new Action<bool>(SetBusy), busy); return; }
            btnStart.Enabled = !busy;
            btnBrowse.Enabled = !busy;
            numQuality.Enabled = !busy;
            chkRecursive.Enabled = !busy;
            chkDelete.Enabled = !busy;
            btnOpen.Enabled = !busy;
        }

        private void StartClicked(object sender, EventArgs e)
        {
            if (!Directory.Exists(folder))
            {
                MessageBox.Show(this, "文件夹不存在：\r\n" + folder, "无法开始", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                return;
            }

            bool rec = chkRecursive.Checked;
            bool del = chkDelete.Checked;
            int total = Converter.Count(folder, rec);
            if (total == 0)
            {
                MessageBox.Show(this, "该文件夹里没有找到 HEIC / HEIF 文件。", "无需转换",
                                MessageBoxButtons.OK, MessageBoxIcon.Information);
                return;
            }

            if (del)
            {
                DialogResult dr = MessageBox.Show(this,
                    string.Format("即将转换 {0} 个文件，并删除 {0} 个原始 HEIC 文件。\r\n\r\n删除不可撤销，确定继续吗？", total),
                    "请确认", MessageBoxButtons.YesNo, MessageBoxIcon.Warning, MessageBoxDefaultButton.Button2);
                if (dr != DialogResult.Yes) return;
            }

            int quality = (int)numQuality.Value;
            txtLog.Clear();
            bar.Value = 0;
            SetBusy(true);
            lblStatus.Text = "转换中…";
            lblStatus.ForeColor = Color.FromArgb(0, 102, 204);

            Thread th = new Thread(delegate ()
            {
                finalCode = Converter.Run(folder, quality, rec, del,
                    log: Log,
                    progress: delegate (int i, int n)
                    {
                        BeginInvoke(new Action(delegate ()
                        {
                            bar.Maximum = n;
                            bar.Value = Math.Min(i, n);
                            lblStatus.Text = string.Format("转换中… {0}/{1}", i, n);
                        }));
                    });
                BeginInvoke(new Action(delegate ()
                {
                    SetBusy(false);
                    bar.Value = bar.Maximum;
                    lblStatus.Text = finalCode == 0 ? "全部完成" : "完成（有失败项，详见日志）";
                    lblStatus.ForeColor = finalCode == 0 ? Color.FromArgb(0, 128, 0) : Color.FromArgb(200, 0, 0);
                    if (finalCode == 1)
                        MessageBox.Show(this, "部分文件转换失败，详见下方日志。", "完成", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                }));
            });
            th.IsBackground = true;
            th.Start();
        }

        private int finalCode;
    }
}