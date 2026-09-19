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
 public struct Point { public int X,Y; public Point(int x,int y){X=x;Y=y;} }
 public interface Driver { void Check(); void Move(Point p); void Down(); void Up(); void Wait(int ms); }
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
  public static void Run(Driver driver, Point[] points,int[] bounds,int duration) {
   if(points==null||points.Length<2||points.Length>256||duration<0||duration>5000)throw new Exception("invalid_stroke_path");
   if(bounds.Length!=4||bounds[2]<=0||bounds[3]<=0)throw new Exception("invalid_stroke_bounds");
   foreach(var p in points) if(p.X<bounds[0]||p.Y<bounds[1]||(long)p.X>=(long)bounds[0]+bounds[2]||(long)p.Y>=(long)bounds[1]+bounds[3])throw new Exception("stroke_out_of_bounds");
   bool armed=false; Exception failure=null;
   try {
    driver.Check(); driver.Move(points[0]); driver.Check(); armed=true; driver.Down();
    for(int i=1;i<points.Length;i++) { driver.Check(); driver.Move(points[i]); driver.Wait(Math.Max(4,duration/(points.Length-1))); }
   } catch(Exception e) { failure=e; }
   finally { if(armed) { try { driver.Up(); } catch(Exception e) { failure=new Exception("mouse_release_failed: "+e.Message,failure); } } }
   if(failure!=null)throw failure;
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
   public List<string> Events=new List<string>();public int Moves;public int FailMove=-1;public bool Cancel;public bool PartialDown;public bool FailRelease;
   public void Check(){if(Cancel&&Moves>=2)throw new Exception("stroke_cancelled");}
   public void Move(Point p){Events.Add("move:"+p.X+","+p.Y);Moves++;if(Moves==FailMove)throw new Exception("move_failed");}
   public void Down(){Events.Add("down");if(PartialDown)throw new Exception("partial_down");}
   public void Up(){Events.Add("up");if(FailRelease)throw new Exception("release failed");}public void Wait(int ms){Check();}
  }
  public static string Run(){
   var pts=new Point[]{new Point(10,10),new Point(20,20),new Point(30,30)};var bounds=new int[]{0,0,100,100};
   var ok=new Mock();Engine.Run(ok,pts,bounds,20);if(String.Join(";",ok.Events.ToArray())!="move:10,10;down;move:20,20;move:30,30;up")throw new Exception("event order");
   foreach(var m in new Mock[]{new Mock{FailMove=2},new Mock{Cancel=true},new Mock{PartialDown=true}}){bool failed=false;try{Engine.Run(m,pts,bounds,20);}catch{failed=true;}if(!failed||m.Events[m.Events.Count-1]!="up")throw new Exception("release guarantee");}
   var invalid=new Mock();try{Engine.Run(invalid,new Point[]{new Point(10,10),new Point(100,20)},bounds,20);}catch{}if(invalid.Events.Count!=0)throw new Exception("boundary before input");
   var release=new Mock{Cancel=true,FailRelease=true};bool reported=false;try{Engine.Run(release,pts,bounds,20);}catch(Exception e){reported=e.Message.Contains("mouse_release_failed");}if(!reported)throw new Exception("release failure must take precedence over cancellation");
   if(IdentityCheck.Difference(1,1,2,2,bounds,bounds,144,144,true,true)!="")throw new Exception("matching identity rejected");
   string mismatch=IdentityCheck.Difference(1,9,2,8,bounds,new int[]{0,0,150,150},96,144,true,true);
   foreach(string field in new string[]{"foreground expected=1 actual=9","pid expected=2 actual=8","dpi expected=96 actual=144","rect expected=[0,0,100,100] actual=[0,0,150,150]"})if(!mismatch.Contains(field))throw new Exception("missing identity diagnostic: "+field);
   return "mock-path-cancel-failure-release:ok";
  }
 }
}
