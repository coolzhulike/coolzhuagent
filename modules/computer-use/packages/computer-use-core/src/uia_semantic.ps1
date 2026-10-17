# 此固定入口只由受监督 helper 执行；请求内容只作数据。
$issued=$false
$pattern=$null
function CheckTargetWindow {
    [CoolzhuNative.Deadline]::Check()
    if((Test-Path -LiteralPath ([string]$r.cancel_file)) -or $watch.ElapsedMilliseconds -ge [long]$r.self_watchdog_ms){throw 'stroke_cancelled: UIA deadline or cancellation'}
    $i=$r.identity
    if(!$i){throw 'uia_target_missing: window identity'}
    [CoolzhuNative.UiaIdentity]::Check([long]$i.handle,[uint32]$i.process_id,[int[]]$i.rect,[uint32]$i.dpi)
}
try {
    if($r.mode -cne 'uia_invoke'){throw 'unsupported_native_mode'}
    CheckTargetWindow
    $expected=[int[]]$r.params.runtime_id
    if(!$expected -or $expected.Length -gt 128){throw 'uia_target_missing: runtime id'}
    $pattern=[CoolzhuNative.UiaInvoke]::Prepare([long]$r.identity.handle,[int]$r.identity.process_id,$expected,[string]$r.params.name)
    CheckTargetWindow
    $pattern.CheckElement([int]$r.identity.process_id,[string]$r.params.name)
    # 进入 COM 调用后失联只能保留过程态，不能写成零输入的终态。
    $progress.Report('in_flight',$false,0,$false,@(),@(),$false,$null)
    $issued=$true
    $pattern.Invoke()
    $progress.Report('final',$false,1,$true,@(),@(),$false,$true)
    'native-input:uia-invoke-ok'
} catch {
    if(!$issued){$progress.Report('final',$false,0,$false,@(),@(),$false,$true)}
    throw
} finally {
    if($null -ne $pattern){$pattern.Dispose()}
}
