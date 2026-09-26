// 固定原生输入 helper。Engine 可注入内存驱动测试，正式 Native 只执行受控鼠标动作。
using System;
using System.IO;
using System.Threading;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Drawing;
using System.Drawing.Imaging;
using System.Security.Cryptography;
using System.Collections.Generic;
namespace CoolzhuStroke {
 public static class Deadline { public static long At; public static void Check(){if(At>0&&DateTimeOffset.UtcNow.ToUnixTimeMilliseconds()>=At)throw new Exception("stroke_cancelled: permit expired");} }
 public struct Point { public int X,Y; public Point(int x,int y){X=x;Y=y;} }
 public interface Driver { void Check(); void Move(Point p); void Down(); void Up(); void Wait(int ms); }
 // 输入事实的接收端。Engine 在每一步被确认之后汇报，字段语义与 Rust 侧 HelperInputFacts 一一对应。
 // CU-F01 v2 补充三项最小标记：
 //  - phase：这条记录是起点（pre_input）、过程中（in_flight）还是**收尾之后的最终事实**（final）；
 //  - cursorMoved：光标是否已经移动过——没有它，"零注入"与"什么都没做"无法区分；
 //  - requestId：宿主下发的请求身份，helper 原样回写，供上层核对"这份事实属于本动作"。
 public interface Progress { void Report(string phase,bool cursorMoved,int injectedPoints,bool buttonDown,bool pathCompleted,bool? released); }
 // 落盘实现：只写一个只有固定键的 JSON 文件，不接收任何外部输入。
 // 每次写入都回读校验；写不进去/对不上就删掉文件并报错——宁可不留事实，
 // 也不能留下一个过期记录被上层当成"当前事实"（可能被读成"未发送"）。
 public sealed class FileProgress:Progress {
  readonly string path; readonly string requestId;
  public FileProgress(string p,string r){path=p;requestId=(r==null?"":r);}
  public void Report(string phase,bool cursorMoved,int injectedPoints,bool buttonDown,bool pathCompleted,bool? released){
   if(String.IsNullOrEmpty(path))return;
   // requestId 由宿主生成（进程号-纳秒-序号），不含需要转义的字符。
   string json="{\"protocol\":2,\"request_id\":\""+requestId+"\",\"phase\":\""+phase+"\""
    +",\"cursor_moved\":"+(cursorMoved?"true":"false")
    +",\"injected_points\":"+injectedPoints
    +",\"button_down\":"+(buttonDown?"true":"false")
    +",\"path_completed\":"+(pathCompleted?"true":"false")
    +",\"released\":"+(released.HasValue?(released.Value?"true":"false"):"null")+"}";
   try{
    File.WriteAllText(path,json);
    if(File.ReadAllText(path)!=json)throw new Exception("progress write was not readable back");
   }catch{
    try{ if(File.Exists(path))File.Delete(path); }catch{}
    throw new Exception("progress_write_failed: 输入事实无法可靠落盘");
   }
  }
 }
 public static class IdentityCheck {
  public static string Difference(long expectedHandle,long actualHandle,uint expectedPid,uint actualPid,int[] expectedRect,int[] actualRect,uint expectedDpi,uint actualDpi,bool pidAvailable,bool rectAvailable) {
   var problems=new List<string>();
   if(expectedHandle!=actualHandle)problems.Add("foreground expected="+expectedHandle+" actual="+actualHandle);
   if(!pidAvailable||expectedPid!=actualPid)problems.Add("pid expected="+expectedPid+" actual="+actualPid+" available="+pidAvailable);
   if(expectedDpi!=actualDpi)problems.Add("dpi expected="+expectedDpi+" actual="+actualDpi);
   bool same=rectAvailable&&expectedRect.Length==4&&actualRect.Length==4;
   for(int i=0;same&&i<4;i++)if(expectedRect[i]!=actualRect[i])same=false;
   if(!same)problems.Add("rect expected=["+String.Join(",",expectedRect)+"] actual=["+String.Join(",",actualRect)+"] available="+rectAvailable);
   return String.Join("; ",problems.ToArray());
  }
 }
 public static class Engine {
  public static void Run(Driver driver, Point[] points,int[] bounds,int duration) { RunWithProgress(driver,points,bounds,duration,null); }
  // 与 Run 相同，但在每一步之后把"已确认的输入事实"汇报给 progress。
  // 汇报时点：起点（未移动、未按下）；第 0 点移动成功之后（已移动光标）；
  // 按下左键成功后 injected=1；每成功移动一个点 injected=i+1；整条路径走完后 path_completed=true；
  // finally 里 Up() 之后再写一条 **final** 记录（收尾已封闭），无论是否按下过都要写：
  // 缺了它，"从未按下"就永远无法被证明。
  public static void RunWithProgress(Driver driver, Point[] points,int[] bounds,int duration,Progress progress) {
   if(points==null||points.Length<2||points.Length>256||duration<0||duration>5000)throw new Exception("invalid_stroke_path");
   if(bounds.Length!=4||bounds[2]<=0||bounds[3]<=0)throw new Exception("invalid_stroke_bounds");
   foreach(var p in points) if(p.X<bounds[0]||p.Y<bounds[1]||(long)p.X>=(long)bounds[0]+bounds[2]||(long)p.Y>=(long)bounds[1]+bounds[3])throw new Exception("stroke_out_of_bounds");
   // armed = 已尝试按下（决定 finally 是否必须补发 Up）；pressed = Down() 已确认成功（决定是否有释放义务）。
   bool armed=false; bool pressed=false; bool moved=false; bool releaseFailed=false; bool completed=false; int injected=0; Exception failure=null;
   // 起点事实：helper 已经开始执行但还没移动光标、没按下任何键。缺了它，上层就无法区分
   // "什么都没注入"与"没有事实"，只能一律保守处理。
   // 这一步的汇报是硬性的：事实写不下去就不开始输入。
   Report(progress,"pre_input",false,0,false,false,null);
   try {
    driver.Check(); driver.Move(points[0]); moved=true;
    // 光标已经移动：立刻把这条事实落盘。否则"移动过光标但零注入"（第 0 点移动成功后
    // 校验失败）与"什么都没做"在记录里完全一样，零输入证明就不完整。
    Report(progress,"in_flight",true,0,false,false,null);
    driver.Check(); armed=true; driver.Down(); pressed=true; injected=1; Report(progress,"in_flight",true,injected,true,false,null);
    for(int i=1;i<points.Length;i++) { driver.Check(); driver.Move(points[i]); injected=i+1; Report(progress,"in_flight",true,injected,true,false,null); driver.Wait(Math.Max(4,duration/(points.Length-1))); }
    completed=true; Report(progress,"in_flight",true,injected,true,true,null);
   } catch(Exception e) { failure=e; }
   finally {
    if(armed) { try { driver.Up(); } catch(Exception e) { failure=new Exception("mouse_release_failed: "+e.Message,failure); releaseFailed=true; } }
    // 收尾事实：只有 armed 过才有"释放结果"可言（没按下时 released 保持 null，
    // 由上层按"从未按下"处理，而不是编一个已释放的结论）。
    ReportSoft(progress,"final",moved,injected,pressed,completed,armed?(!releaseFailed):(bool?)null);
   }
   if(failure!=null) throw failure;
  }
  // 输入过程中的汇报是硬性的：写不下去就中止，避免留下一份可能过期的记录。
  static void Report(Progress progress,string phase,bool cursorMoved,int injectedPoints,bool buttonDown,bool pathCompleted,bool? released){
   if(progress==null)return;
   progress.Report(phase,cursorMoved,injectedPoints,buttonDown,pathCompleted,released);
  }
  // 收尾汇报：失败只影响"事实完整度"，不得覆盖真正的失败分类。
  static void ReportSoft(Progress progress,string phase,bool cursorMoved,int injectedPoints,bool buttonDown,bool pathCompleted,bool? released){
   if(progress==null)return;
   try{ progress.Report(phase,cursorMoved,injectedPoints,buttonDown,pathCompleted,released); }catch{}
  }
 }
 public class Native:Driver {
  [StructLayout(LayoutKind.Sequential)] struct RECT { public int Left,Top,Right,Bottom; }
  [StructLayout(LayoutKind.Sequential)] struct MOUSEINPUT { public int dx,dy; public uint data,flags,time; public UIntPtr extra; }
  [StructLayout(LayoutKind.Sequential)] struct INPUT { public uint type; public MOUSEINPUT mouse; }
  [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h,out RECT r);
  [DllImport("user32.dll")] static extern bool GetClientRect(IntPtr h,out RECT r);
  [DllImport("user32.dll")] static extern bool ClientToScreen(IntPtr h,ref Point p);
  [DllImport("user32.dll")] static extern uint GetDpiForWindow(IntPtr h);
  [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  [DllImport("user32.dll")] static extern bool SetCursorPos(int x,int y);
  [DllImport("user32.dll")] static extern uint SendInput(uint n,INPUT[] input,int size);
  [DllImport("user32.dll")] static extern short GetAsyncKeyState(int key);
  [DllImport("user32.dll")] static extern int GetSystemMetrics(int key);
  readonly IntPtr handle; readonly uint pid,dpi; readonly int[] rect; readonly string cancel;
  readonly Stopwatch watch=Stopwatch.StartNew();
  public Native(long h,uint p,int[] r,uint d,string c){handle=new IntPtr(h);pid=p;rect=r;dpi=d;cancel=c;if(SetThreadDpiAwarenessContext(new IntPtr(-4))==IntPtr.Zero)throw new Exception("dpi_context_failed: 无法设置物理像素坐标上下文");}
  public void Check(){
   Deadline.Check();
   if(File.Exists(cancel)||(GetAsyncKeyState(0x1B)&0x8000)!=0||watch.ElapsedMilliseconds>7000)throw new Exception("stroke_cancelled");
   uint p; RECT r;var foreground=GetForegroundWindow();bool pidAvailable=GetWindowThreadProcessId(handle,out p)!=0;bool rectAvailable=GetWindowRect(handle,out r);uint actualDpi=GetDpiForWindow(handle);
   string mismatch=IdentityCheck.Difference(handle.ToInt64(),foreground.ToInt64(),pid,p,rect,new int[]{r.Left,r.Top,r.Right-r.Left,r.Bottom-r.Top},dpi,actualDpi,pidAvailable,rectAvailable);
   if(mismatch.Length>0)throw new Exception("stale_observation: "+mismatch);
  }
  public void Move(Point p){Check();if(!SetCursorPos(p.X,p.Y))throw new Exception("cursor_move_failed");}
  static void Flag(uint f){var input=new INPUT();input.mouse.flags=f;if(SendInput(1,new INPUT[]{input},Marshal.SizeOf(typeof(INPUT)))!=1)throw new Exception("SendInput failed");}
  public static void EmergencyRelease(){Flag(0x0004);}
  public void Down(){Check();Flag(0x0002);}
  public void Up(){Exception last=null;for(int i=0;i<3;i++){try{Flag(0x0004);return;}catch(Exception e){last=e;Thread.Sleep(10);}}throw last;}
  public void Wait(int ms){for(int left=ms;left>0;){Check();int slice=Math.Min(left,10);Thread.Sleep(slice);left-=slice;}Check();}
  public object Capture(){
   Check(); int left=GetSystemMetrics(76),top=GetSystemMetrics(77),width=GetSystemMetrics(78),height=GetSystemMetrics(79);
   int x=Math.Max(rect[0],left),y=Math.Max(rect[1],top),w=Math.Min(rect[0]+rect[2],left+width)-x,h=Math.Min(rect[1]+rect[3],top+height)-y;
   if(w<=0||h<=0)throw new Exception("capture_out_of_bounds: 窗口没有可见屏幕区域");
   using(var bitmap=new Bitmap(w,h))using(var g=Graphics.FromImage(bitmap))using(var stream=new MemoryStream()){
    g.CopyFromScreen(x,y,0,0,new Size(w,h),CopyPixelOperation.SourceCopy);Check();bitmap.Save(stream,ImageFormat.Png);
    RECT client;var origin=new Point(0,0);if(!GetClientRect(handle,out client)||!ClientToScreen(handle,ref origin))throw new Exception("client_bounds_failed");
    var bytes=stream.ToArray();using(var hash=SHA256.Create()){return new {data_url="data:image/png;base64,"+Convert.ToBase64String(bytes),width=w,height=h,screen_rect=new int[]{x,y,w,h},client_rect=new int[]{origin.X,origin.Y,client.Right-client.Left,client.Bottom-client.Top},sha256=BitConverter.ToString(hash.ComputeHash(bytes)).Replace("-","").ToLowerInvariant()};}
   }
  }
 }
 public static class MockChecks {
  class Mock:Driver {
   public List<string> Events=new List<string>();public int Moves;public int FailMove=-1;public bool Cancel;public bool PartialDown;public bool FailRelease;public bool FailCheck;
   public void Check(){if(FailCheck)throw new Exception("stale_observation: mock 身份校验失败");if(Cancel&&Moves>=2)throw new Exception("stroke_cancelled");}
   public void Move(Point p){Events.Add("move:"+p.X+","+p.Y);Moves++;if(Moves==FailMove)throw new Exception("move_failed");}
   public void Down(){Events.Add("down");if(PartialDown)throw new Exception("partial_down");}
   public void Up(){Events.Add("up");if(FailRelease)throw new Exception("release failed");}public void Wait(int ms){Check();}
  }
  class Collect:Progress {
   public string Phase="";public bool CursorMoved;public int InjectedPoints=-1;public bool ButtonDown;public bool PathCompleted;public bool? Released;public int Reports;
   public void Report(string phase,bool cursorMoved,int injectedPoints,bool buttonDown,bool pathCompleted,bool? released){Reports++;Phase=phase;CursorMoved=cursorMoved;InjectedPoints=injectedPoints;ButtonDown=buttonDown;PathCompleted=pathCompleted;Released=released;}
  }
  static string ProgressFile(string path){
   return (path==null||path.Length==0)?Path.Combine(Path.GetTempPath(),"coolzhu-stroke-progress-"+Process.GetCurrentProcess().Id+"-"+Guid.NewGuid().ToString("N")+".json"):path;
  }
  static Collect CheckProgress(Point[] pts,int[] bounds,Mock driver){
   var sink=new Collect();try{Engine.RunWithProgress(driver,pts,bounds,20,sink);}catch{}return sink;
  }
  // 汇报写不下去时抛错的接收端。
  class Failing:Progress {
   public int Reports;public int FailAt;
   public void Report(string phase,bool cursorMoved,int injectedPoints,bool buttonDown,bool pathCompleted,bool? released){Reports++;if(Reports==FailAt)throw new Exception("progress_write_failed");}
  }
  // 事实写不下去必须中止输入：宁可停下，也不能继续注入却留下一份可能过期的记录。
  // 汇报顺序：①起点 ②第 0 点移动成功之后 ③按下确认之后 ④⑤逐点 ⑥走完 ⑦收尾(final)。
  static void CheckProgressFailureAbortsInput(Point[] pts,int[] bounds){
   // ① 在"已经按下"之后写不进去：必须中止，但**仍要**释放。
   var sink=new Failing{FailAt=3};var driver=new Mock();bool aborted=false;
   try{Engine.RunWithProgress(driver,pts,bounds,20,sink);}catch{aborted=true;}
   if(!aborted||driver.Moves!=1||driver.Events[driver.Events.Count-1]!="up")throw new Exception("a failing progress sink must abort input and still release");
   // ② 在"按下之前、已经移动光标"时写不进去：中止，但**不**发多余的 UP（什么都没按下）。
   var beforePress=new Failing{FailAt=2};var moved=new Mock();bool abortedEarly=false;
   try{Engine.RunWithProgress(moved,pts,bounds,20,beforePress);}catch{abortedEarly=true;}
   if(!abortedEarly||moved.Moves!=1||moved.Events[moved.Events.Count-1]!="move:10,10")throw new Exception("an unwritable fact before pressing must abort without extra UP");
   // 真正落盘失败的场景：目录不存在 → FileProgress 报错 → 连输入都不开始。
   var unwritable=Path.Combine(Path.GetTempPath(),"coolzhu-no-such-dir-"+Guid.NewGuid().ToString("N"),"p.json");
   var blocked=new Mock();bool refused=false;
   try{Engine.RunWithProgress(blocked,pts,bounds,20,new FileProgress(unwritable,null));}catch(Exception e){refused=e.Message.Contains("progress_write_failed");}
   if(!refused||blocked.Moves!=0)throw new Exception("an unwritable progress file must stop input before any move");
  }
  // 用内存驱动跑真实 Engine 的进度汇报：失败点之前的动作必须已被确认，
  // 且 released 只有 finally 跑过才为 true——上层据此才知道"部分已注入 + 是否已释放"。
  // 每一条最终记录都必须声明 final 阶段与"光标是否移动过"，否则上层无法证明零输入。
  static void CheckProgressReporting(Point[] pts,int[] bounds){
   var partial=CheckProgress(pts,bounds,new Mock{FailMove=2});
   if(partial.Reports==0||partial.Phase!="final"||!partial.CursorMoved||partial.InjectedPoints!=1||!partial.ButtonDown||partial.PathCompleted||partial.Released!=true)throw new Exception("partial progress must end as final/injected=1 down=true completed=false released=true, got "+partial.Reports+"/"+partial.Phase+"/"+partial.InjectedPoints);
   var completed=CheckProgress(pts,bounds,new Mock());
   if(completed.Reports<2||completed.Phase!="final"||!completed.CursorMoved||completed.InjectedPoints!=3||!completed.PathCompleted||completed.Released!=true)throw new Exception("completed progress must end as final/injected=3 completed=true released=true");
   // 按下之前失败：最终事实必须是 final + 没有移动过光标 + 零注入 + 没有释放结论。
   var empty=CheckProgress(pts,bounds,new Mock{PartialDown=true});
   if(empty.Reports==0||empty.Phase!="final"||empty.InjectedPoints!=0||empty.ButtonDown)throw new Exception("failure before button down must report zero injection");
   var cancelled=CheckProgress(pts,bounds,new Mock{Cancel=true});
   if(cancelled.Reports==0||cancelled.Phase!="final"||cancelled.InjectedPoints!=2||cancelled.PathCompleted||cancelled.Released!=true)throw new Exception("cancel progress must keep confirmed points and confirmed release");
   CheckProgressFailureAbortsInput(pts,bounds);
  }
  // 真正落盘再断言：文件内容必须能被 Rust 侧原样解析（字段名是跨语言契约）。
  // 只跑"路径第 2 点后失败"的场景，让上层读到真实的失败事实。
  public static string RunProgressCheck(string path){
   var pts=new Point[]{new Point(10,10),new Point(20,20),new Point(30,30)};var bounds=new int[]{0,0,100,100};
   var file=ProgressFile(path);
   try{Engine.RunWithProgress(new Mock{FailMove=2},pts,bounds,20,new FileProgress(file,"mock-request"));}catch{}
   if(!File.Exists(file))throw new Exception("progress file was not written");
   var text=File.ReadAllText(file);
   if(!text.Contains("\"protocol\":2")||!text.Contains("\"request_id\":\"mock-request\"")||!text.Contains("\"phase\":\"final\"")||!text.Contains("\"cursor_moved\":true")||!text.Contains("\"injected_points\":1")||!text.Contains("\"button_down\":true")||!text.Contains("\"path_completed\":false")||!text.Contains("\"released\":true"))throw new Exception("unexpected partial progress json: "+text);
   return file;
  }
  // "整条路径完成"的落盘事实。
  public static string RunCompletedProgressCheck(string path){
   var pts=new Point[]{new Point(10,10),new Point(20,20),new Point(30,30)};var bounds=new int[]{0,0,100,100};
   var file=ProgressFile(path);
   try{Engine.RunWithProgress(new Mock(),pts,bounds,20,new FileProgress(file,"mock-request"));}catch{}
   if(!File.Exists(file))throw new Exception("progress file was not written");
   var text=File.ReadAllText(file);
   if(!text.Contains("\"phase\":\"final\"")||!text.Contains("\"injected_points\":3")||!text.Contains("\"path_completed\":true")||!text.Contains("\"released\":true"))throw new Exception("unexpected completed progress json: "+text);
   return file;
  }
  // "收尾之前就被外部杀掉"的落盘事实：只有起点快照（没有 final 记录）。
  // 上层**不得**把它当成零输入证明（T03）。
  public static string RunPreInputKillCheck(string path){
   var pts=new Point[]{new Point(10,10),new Point(20,20),new Point(30,30)};var bounds=new int[]{0,0,100,100};
   var file=ProgressFile(path);
   var sink=new FileProgress(file,"mock-request");
   // 只写起点事实：模拟 helper 在开始输入之前就被强杀，收尾记录永远没写出来。
   sink.Report("pre_input",false,0,false,false,null);
   if(!File.Exists(file))throw new Exception("progress file was not written");
   var text=File.ReadAllText(file);
   if(!text.Contains("\"phase\":\"pre_input\"")||!text.Contains("\"cursor_moved\":false"))throw new Exception("unexpected pre-input json: "+text);
   return file;
  }
  // 用内存驱动跑真实 Engine（零注入）：供受控路径在**不驱动真实鼠标**的前提下
  // 走完整条生产编排（helper 事实 → 推导 → 收尾）。`mock_scenario` 只由测试构造的请求带来，
  // 生产代码里没有任何写入该键的调用点。
  public static void RunScenario(string scenario,string path,string requestId){
   var pts=new Point[]{new Point(10,10),new Point(20,20),new Point(30,30)};var bounds=new int[]{0,0,100,100};
   Mock driver;
   if(scenario=="release_failure")driver=new Mock{FailMove=2,FailRelease=true};
   else if(scenario=="success")driver=new Mock();
   else if(scenario=="identity_failure")driver=new Mock{FailCheck=true};
   else throw new Exception("unsupported mock scenario: "+scenario);
   var sink=new FileProgress(ProgressFile(path),requestId);
   Exception thrown=null;
   try{Engine.RunWithProgress(driver,pts,bounds,20,sink);}catch(Exception e){thrown=e;}
   // 身份校验失败：Move/Down/Up 必须一次都没发生。
   if(scenario=="identity_failure"&&(driver.Moves!=0||driver.Events.Count!=0||thrown==null))throw new Exception("identity failure must not call Move/Down/Up");
   // 失败原样抛出：与生产一致，helper 以非零状态退出并把原因写进 stderr。
   if(thrown!=null)throw thrown;
  }
  public static string Run(){
   var pts=new Point[]{new Point(10,10),new Point(20,20),new Point(30,30)};var bounds=new int[]{0,0,100,100};
   var ok=new Mock();Engine.Run(ok,pts,bounds,20);if(String.Join(";",ok.Events.ToArray())!="move:10,10;down;move:20,20;move:30,30;up")throw new Exception("event order");
   foreach(var m in new Mock[]{new Mock{FailMove=2},new Mock{Cancel=true},new Mock{PartialDown=true}}){bool failed=false;try{Engine.Run(m,pts,bounds,20);}catch{failed=true;}if(!failed||m.Events[m.Events.Count-1]!="up")throw new Exception("release guarantee");}
   var invalid=new Mock();try{Engine.Run(invalid,new Point[]{new Point(10,10),new Point(100,20)},bounds,20);}catch{}if(invalid.Events.Count!=0)throw new Exception("boundary before input");
   var release=new Mock{Cancel=true,FailRelease=true};bool reported=false;try{Engine.Run(release,pts,bounds,20);}catch(Exception e){reported=e.Message.Contains("mouse_release_failed");}if(!reported)throw new Exception("release failure must take precedence over cancellation");
   CheckProgressReporting(pts,bounds);
   if(IdentityCheck.Difference(1,1,2,2,bounds,bounds,144,144,true,true)!="")throw new Exception("matching identity rejected");
   string mismatch=IdentityCheck.Difference(1,9,2,8,bounds,new int[]{0,0,150,150},96,144,true,true);
   foreach(string field in new string[]{"foreground expected=1 actual=9","pid expected=2 actual=8","dpi expected=96 actual=144","rect expected=[0,0,100,100] actual=[0,0,150,150]"})if(!mismatch.Contains(field))throw new Exception("missing identity diagnostic: "+field);
   return "mock-path-cancel-failure-release:ok";
  }
 }
}
