// 与观察端共用 Windows 原生 UIA COM。旧 .NET UIA 代理会丢失部分 InvokePattern。
// 接口槽顺序来自 Windows SDK IUIAutomation；占位槽不对外调用。
namespace CoolzhuNative {
 using System;
 using System.Runtime.InteropServices;
 [ComImport,Guid("30cbe57d-d9d0-452a-ab13-7ac5ac4825ee"),InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
 interface NativeAutomation {
  void CompareElements(); void CompareRuntimeIds(); void GetRootElement();
  NativeElement ElementFromHandle(IntPtr hwnd);
  void ElementFromPoint(); void GetFocusedElement(); void GetRootElementBuildCache();
  void ElementFromHandleBuildCache(); void ElementFromPointBuildCache(); void GetFocusedElementBuildCache();
  void CreateTreeWalker(); void ControlViewWalker(); void ContentViewWalker(); void RawViewWalker();
  void RawViewCondition(); void ControlViewCondition(); void ContentViewCondition(); void CreateCacheRequest();
  void CreateTrueCondition(); void CreateFalseCondition();
  [return:MarshalAs(UnmanagedType.Interface)] object CreatePropertyCondition(int property,[MarshalAs(UnmanagedType.Struct)] object value);
 }
 [ComImport,Guid("d22108aa-8ac5-49a5-837b-37bbb3d7591e"),InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
 interface NativeElement {
  void SetFocus(); void GetRuntimeId(); void FindFirst();
  NativeElements FindAll(int scope,[MarshalAs(UnmanagedType.Interface)] object condition);
  void FindFirstBuildCache(); void FindAllBuildCache(); void BuildUpdatedCache();
  [return:MarshalAs(UnmanagedType.Struct)] object GetCurrentPropertyValue(int property);
  void GetCurrentPropertyValueEx(); void GetCachedPropertyValue(); void GetCachedPropertyValueEx();
  [return:MarshalAs(UnmanagedType.Interface)] object GetCurrentPatternAs(int pattern,ref Guid iid);
 }
 [ComImport,Guid("14314595-b4bc-4055-95f2-58f2e42c9855"),InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
 interface NativeElements { int Length(); NativeElement GetElement(int index); }
 [ComImport,Guid("fb377fbe-8ea6-46d5-9c73-6499642d3059"),InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
 interface NativeInvoke { void Invoke(); }
 public sealed class UiaInvoke : IDisposable {
  NativeAutomation automation; object condition; NativeElement root,element; NativeElements elements; NativeInvoke pattern;
  public static UiaInvoke Prepare(long hwnd,int pid,int[] runtimeId,string name) {
   UiaInvoke operation=new UiaInvoke();
   try {
    Deadline.Check();
    operation.automation=(NativeAutomation)Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid("ff48dba4-60ef-4201-aa87-54103eef594e")));
    operation.root=operation.automation.ElementFromHandle(new IntPtr(hwnd));
    operation.condition=operation.automation.CreatePropertyCondition(30000,runtimeId);
    operation.elements=operation.root.FindAll(7,operation.condition);
    if(operation.elements.Length()!=1)throw new Exception("uia_target_missing_or_ambiguous");
    operation.element=operation.elements.GetElement(0);
    operation.CheckElement(pid,name);
    Guid iid=new Guid("fb377fbe-8ea6-46d5-9c73-6499642d3059");
    operation.pattern=(NativeInvoke)operation.element.GetCurrentPatternAs(10000,ref iid);
    if(operation.pattern==null)throw new Exception("uia_pattern_unavailable");
    return operation;
   } catch {operation.Dispose();throw;}
  }
  public void CheckElement(int pid,string name) {
   Deadline.Check();
   if(Convert.ToInt32(element.GetCurrentPropertyValue(30002))!=pid
     || !Convert.ToBoolean(element.GetCurrentPropertyValue(30010))
     || Convert.ToBoolean(element.GetCurrentPropertyValue(30022))
     || Convert.ToBoolean(element.GetCurrentPropertyValue(30019))
     || !String.Equals(element.GetCurrentPropertyValue(30005) as string,name,StringComparison.Ordinal))
    throw new Exception("stale_observation: UIA target changed");
  }
  public void Invoke(){pattern.Invoke();}
  static void Release(object value){if(value!=null && Marshal.IsComObject(value))Marshal.ReleaseComObject(value);}
  public void Dispose(){Release(pattern);pattern=null;Release(element);element=null;Release(elements);elements=null;Release(condition);condition=null;Release(root);root=null;Release(automation);automation=null;}
 }
}
