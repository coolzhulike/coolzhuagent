//! UIA 语义调用复用原生 helper 的授权、Job、截止期限和回执流程。
//! 固定脚本只接受 RuntimeId，不把控件名称或模型文本解释成命令。
use super::*;

pub(super) fn invoke(
    runtime_id: &[i32],
    name: &str,
    attempt: &NativeInputAttempt,
    timeout: Duration,
) -> Result<NativeInputOutcome, NativeInputFailure> {
    if attempt.window.is_none() || runtime_id.is_empty() || runtime_id.len() > 128 {
        return Err(NativeInputFailure::before_input("uia_target_missing: 缺少冻结窗口或控件 RuntimeId"));
    }
    if name.len() > 8192 {
        return Err(NativeInputFailure::before_input("uia_target_invalid: 控件名称过长"));
    }
    native_run("uia_invoke", serde_json::json!({"runtime_id": runtime_id, "name": name}),
        ReleaseObligation::none(), attempt, timeout, InputBackend::SendInput)
}

pub(super) fn helper_script() -> String {
    // 共用事实序列化和两相授权，避免 UIA 路径形成第二套许可协议。
    let common = NATIVE_INPUT_HELPER_CS.split(" public static class Buttons {").next().unwrap();
    let start = NATIVE_INPUT_HELPER_ENTRY.find("if($r.two_phase_helper){").unwrap();
    let end = NATIVE_INPUT_HELPER_ENTRY.find("if($sp[1])").unwrap();
    let handshake = &NATIVE_INPUT_HELPER_ENTRY[start..end];
    format!("$ErrorActionPreference='Stop'; $OutputEncoding=[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false);\nAdd-Type -TypeDefinition @'\n{common}{UIA_IDENTITY}\n{UIA_COM}\n'@\n{PREFIX}\n{handshake}\n{BODY}")
}

const UIA_IDENTITY: &str = r#"
 public static class UiaIdentity {
  [StructLayout(LayoutKind.Sequential)] struct RECT { public int L,T,R,B; }
  [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h,out RECT r);
  [DllImport("user32.dll")] static extern uint GetDpiForWindow(IntPtr h);
  [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  public static void Check(long h,uint p,int[] r,uint d) {
   if(SetThreadDpiAwarenessContext(new IntPtr(-4))==IntPtr.Zero)throw new Exception("dpi_context_failed");
   IntPtr hwnd=new IntPtr(h); uint actual; RECT rect;
   bool hasPid=GetWindowThreadProcessId(hwnd,out actual)!=0;
   bool hasRect=GetWindowRect(hwnd,out rect);
   string diff=NativeIdentity.Difference(h,GetForegroundWindow().ToInt64(),p,actual,r,new int[]{rect.L,rect.T,rect.R-rect.L,rect.B-rect.T},d,GetDpiForWindow(hwnd),hasPid,hasRect);
   if(diff.Length>0)throw new Exception("stale_observation: "+diff);
  }
 }
}
"#;

const PREFIX: &str = r#"
$r=[Console]::In.ReadToEnd()|ConvertFrom-Json
$progress=New-Object CoolzhuNative.FileProgress ([string]$r.progress_file,[string]$r.request_id)
$progress.Report('pre_input',$false,0,$false,@(),@(),$false,$null)
$watch=[Diagnostics.Stopwatch]::StartNew()
"#;

const BODY: &str = include_str!("uia_semantic.ps1");
const UIA_COM: &str = include_str!("uia_semantic.cs");

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn semantic_helper_is_bounded_and_cannot_fall_back_to_coordinates() {
        let script=helper_script();
        let command=native_helper_command(&script);
        // 对每个UTF-16单元都预留双写转义，比实际命令行更保守。
        let upper_bound=command.get_args().chain(std::iter::once(command.get_program()))
            .map(|arg| arg.to_string_lossy().encode_utf16().count()*2+4).sum::<usize>()+1;
        assert!(upper_bound < 31_000);
        assert!(script.find("if($r.two_phase_helper){").unwrap() < script.find("$pattern.Invoke()").unwrap());
        for forbidden in ["Invoke-Expression", "SendInput", "SetCursorPos", "[CoolzhuNative.Engine]::Run"] {
            assert!(!script.contains(forbidden));
        }
        let cancelled=||false;
        let attempt=NativeInputAttempt::without_identity(&cancelled);
        assert!(invoke(&[1],"button",&attempt,Duration::from_secs(1)).is_err());
    }
}
