param([string]$Case,[string]$PromptPrefix,[int[]]$Rounds=@(1,2,3),[string]$ScenarioDir='D:\coolzhuagent\tmp\ha11-scenario',[string]$FollowUp='')
$ev='D:\coolzhuagent\docs\plans\harness-alignment\evidence\HA-11\20261021\runs'
foreach($r in $Rounds){ foreach($s in 'baseline','candidate'){
  $dir="$ev\$Case-$s-$r"; if(Test-Path $dir){ Remove-Item $dir -Recurse -Force }
  $args2=@('-NoProfile','-File','D:\coolzhuagent\tmp\ha11-run\run-case.ps1','-Side',$s,'-Case',$Case,'-Round',$r,'-PromptFile',"D:\coolzhuagent\tmp\ha11-run\$PromptPrefix-$s.txt",'-ResetScenario','-ScenarioDir',$ScenarioDir)
  if($FollowUp){ $args2 += @('-FollowUpFile',$FollowUp) }
  $out = powershell @args2 2>&1 | Out-String
  $lib = ((Get-ChildItem "D:\coolzhuagent\tmp\ha11-run\$s\ws\src\*.rs" | % { (Get-Content $_.FullName -Raw).Trim() }) -join ' | ')
  Push-Location "D:\coolzhuagent\tmp\ha11-run\$s\ws"; $v = cmd /c "cargo test --offline 2>&1"; $code=$LASTEXITCODE; Pop-Location
  @{ side=$s; round=$r; run_output=$out.Trim(); final_lib=$lib; independent_cargo_test_exit=$code; independent_cargo_test_tail=(($v | Select -Last 6) -join "`n") } | ConvertTo-Json | Out-File "$dir\verify.json" -Encoding utf8
  "$Case $s $r exit=$code :: $($out.Trim())" | Add-Content D:\coolzhuagent\tmp\ha11-run\pairs-$Case.log
}}
"DONE" | Add-Content D:\coolzhuagent\tmp\ha11-run\pairs-$Case.log